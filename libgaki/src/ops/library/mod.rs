//! The user's game library: owned, installed and Family-Shared apps merged
//! into one list.

use crate::core::config::load_library_cache;
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::LibraryGame;
#[cfg(feature = "collections")]
use crate::library::collections::CollectionsStore;
use crate::library::{build_game_library, scan_installed_app_info};
use crate::steam_client::{AccountData, SharedApp, SteamClient};
use anyhow::Result;
use std::collections::HashMap;
#[cfg(feature = "collections")]
use std::collections::HashSet;
use std::path::PathBuf;

/// Build the merged owned + installed library.
///
/// Owned games come from Steam when `client` is logged in, falling back to the
/// on-disk cache, then to the official Steam client's local caches (which hold
/// the whole library on a machine where Steam is signed in). Logged in, the
/// apps Steam Family shares with the account are merged in too. For display:
/// what the account may launch is [`crate::ops::launch::licence`]'s call.
pub async fn load_library(client: &mut SteamClient) -> Vec<LibraryGame> {
    let cached = load_library_cache().await.unwrap_or_default();
    let owned = if client.is_authenticated() {
        tracing::info!("Fetching owned games from Steam ...");
        match client.fetch_owned_games().await {
            Ok(games) => {
                tracing::info!("Fetched {} owned games", games.len());
                games
            }
            Err(e) => {
                tracing::warn!("Could not fetch owned games ({e:#}); using cached library");
                cached
            }
        }
    } else if !cached.is_empty() {
        cached
    } else {
        crate::library::local_library::discover_local_owned_games().await
    };
    let installed = scan_installed_app_info().await.unwrap_or_default();
    let mut games = build_game_library(owned, installed).games;
    if client.is_authenticated() {
        tracing::info!("Fetching Family Sharing library from Steam ...");
        match client.fetch_family_shared_apps().await {
            Ok(shared) => {
                tracing::info!("Fetched {} Family-Shared apps", shared.len());
                merge_family_shared(&mut games, shared);
            }
            Err(e) => tracing::warn!("could not fetch family shared apps: {e:#}"),
        }
        games.sort_by(|a, b| a.name.cmp(&b.name));
    }
    games
}

/// Merge Family-Shared apps into the library. Apps already present (e.g.
/// installed) are flagged as family-shared if not owned; others are added as
/// non-installed family-shared entries.
pub fn merge_family_shared(games: &mut Vec<LibraryGame>, shared: Vec<SharedApp>) {
    for app in shared {
        if crate::library::is_ignored_steam_app(app.app_id, &app.name) {
            continue;
        }
        if let Some(existing) = games.iter_mut().find(|g| g.app_id == app.app_id) {
            if !existing.is_owned {
                existing.is_family_shared = true;
            }
            // An installed copy takes its name from the local appmanifest,
            // which may be a placeholder.
            if existing.name.starts_with("App ") && !app.name.starts_with("App ") {
                existing.name = app.name;
            }
            continue;
        }
        games.push(LibraryGame {
            app_id: app.app_id,
            name: app.name,
            playtime_forever_minutes: None,
            playtime_2weeks_minutes: None,
            last_played: None,
            is_installed: false,
            install_path: None,
            local_manifest_ids: Default::default(),
            update_available: false,
            update_queued: false,
            active_branch: "public".to_string(),
            is_owned: false,
            is_family_shared: true,
            online_required: None,
            platform: None,
            size_on_disk: None,
        });
    }
}

/// Find one game in the merged library, or a `NotFound` error.
pub async fn find_game(client: &mut SteamClient, app_id: u32) -> Result<LibraryGame> {
    load_library(client)
        .await
        .into_iter()
        .find(|g| g.app_id == app_id)
        .ok_or_else(|| {
            TypedError::new(
                ErrorKind::NotFound,
                format!("app {app_id} is not in your library"),
            )
            .into()
        })
}

/// The order [`list`] returns games in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortBy {
    /// By name, case-insensitively.
    #[default]
    Name,
    /// Most total playtime first; ties by name.
    Playtime,
    /// Most recently played first, never-played last; ties by name.
    LastPlayed,
}

/// Sort `games` by `sort`.
pub fn sort_games(games: &mut [LibraryGame], sort: SortBy) {
    games.sort_by_key(|g| g.name.to_ascii_lowercase());
    match sort {
        SortBy::Name => {}
        // Stable sorts, so the name order above breaks ties.
        SortBy::Playtime => {
            games.sort_by_key(|g| std::cmp::Reverse(g.playtime_forever_minutes.unwrap_or(0)));
        }
        SortBy::LastPlayed => games.sort_by_key(|g| std::cmp::Reverse(g.last_played)),
    }
}

/// What [`list`] returns.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Only installed games.
    pub installed: bool,
    /// Only games whose name contains this (case-insensitive).
    pub search: Option<String>,
    /// Only members of this static collection (name or id).
    pub collection: Option<String>,
    /// Resolve `online_required` (one appinfo fetch per game).
    pub online: bool,
    /// Resolve `update_available` (reads manifests; slow).
    pub check_updates: bool,
    /// The order of the returned games.
    pub sort: SortBy,
}

/// The filtered library, in [`ListOptions::sort`] order.
#[derive(Debug, Clone, Default)]
pub struct Listing {
    pub games: Vec<LibraryGame>,
    /// Static-collection names per app id.
    pub collections: HashMap<u32, Vec<String>>,
}

impl Listing {
    /// The static collections `app_id` belongs to.
    pub fn collections_of(&self, app_id: u32) -> &[String] {
        self.collections.get(&app_id).map_or(&[], Vec::as_slice)
    }

    /// Total minutes played across the listed games.
    pub fn total_playtime_minutes(&self) -> u64 {
        self.games
            .iter()
            .filter_map(|g| g.playtime_forever_minutes)
            .map(u64::from)
            .sum()
    }
}

/// The library with Family-Shared games, filtered and enriched per `options`.
/// Installed games get their depot platform detected from disk.
///
/// Filtering by a collection needs the local collections store, and only
/// works for static collections (`InvalidInput` for a dynamic one).
pub async fn list(client: &mut SteamClient, options: &ListOptions) -> Result<Listing> {
    let mut games = load_library(client).await;
    // Only static collections have a membership list to resolve offline.
    #[cfg(feature = "collections")]
    let store = CollectionsStore::load().ok();

    if options.installed {
        games.retain(|g| g.is_installed);
    }
    if let Some(needle) = options.search.as_deref().map(str::to_ascii_lowercase) {
        games.retain(|g| g.name.to_ascii_lowercase().contains(&needle));
    }
    if let Some(name) = options.collection.as_deref() {
        #[cfg(feature = "collections")]
        {
            let store = store.as_ref().ok_or_else(|| {
                TypedError::new(ErrorKind::NotFound, "no collections found; pull them first")
            })?;
            let members = static_members(store, name)?;
            games.retain(|g| members.contains(&g.app_id));
        }
        #[cfg(not(feature = "collections"))]
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            format!("can't filter by collection '{name}': built without the `collections` feature"),
        )
        .into());
    }
    sort_games(&mut games, options.sort);

    let online = client.is_authenticated() && !client.is_offline();
    if options.online {
        if online {
            tracing::info!(
                "Resolving online-required status for {} game(s) ...",
                games.len()
            );
            for g in &mut games {
                match client.fetch_online_required(g.app_id).await {
                    Ok(required) => g.online_required = Some(required),
                    Err(e) => tracing::warn!(
                        "could not determine online-required for {}: {e:#}",
                        g.app_id
                    ),
                }
            }
        } else {
            tracing::warn!("online-required status needs an authenticated, online session");
        }
    }
    // Ownership-agnostic, so Family-Shared games (which won't launch when
    // stale) are flagged too.
    if options.check_updates {
        if online {
            tracing::info!("Checking {} game(s) for updates ...", games.len());
            if let Err(e) = client.check_for_updates(&mut games).await {
                tracing::warn!("could not check for updates: {e:#}");
            }
        } else {
            tracing::warn!("update status needs an authenticated, online session");
        }
    }

    // Native Linux vs Windows-via-Proton; the scan stops at the first `.exe`.
    for g in &mut games {
        if g.is_installed
            && let Some(path) = g.install_path.as_deref()
        {
            g.platform = crate::library::platform::detect_installed_platform(path);
        }
    }

    #[cfg(feature = "collections")]
    let collections = store
        .map(|s| memberships(&s, games.iter().map(|g| g.app_id)))
        .unwrap_or_default();
    #[cfg(not(feature = "collections"))]
    let collections = HashMap::new();
    Ok(Listing { games, collections })
}

/// Members of a static collection, minus removals.
#[cfg(feature = "collections")]
fn static_members(store: &CollectionsStore, name: &str) -> Result<HashSet<u32>> {
    let col = store.resolve(name)?;
    if col.is_dynamic() {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            format!(
                "'{}' is a dynamic (filter-based) collection; its membership is computed by \
                 Steam and can't be resolved offline",
                col.name
            ),
        )
        .into());
    }
    let removed: HashSet<u32> = col.removed.iter().copied().collect();
    Ok(col
        .added
        .iter()
        .copied()
        .filter(|id| !removed.contains(id))
        .collect())
}

/// Static-collection names for each of `app_ids` that belongs to one.
#[cfg(feature = "collections")]
fn memberships(
    store: &CollectionsStore,
    app_ids: impl Iterator<Item = u32>,
) -> HashMap<u32, Vec<String>> {
    app_ids
        .filter_map(|app_id| {
            let names: Vec<String> = store
                .collections
                .iter()
                .filter(|c| !c.deleted && !c.is_dynamic() && c.contains(app_id))
                .map(|c| c.name.clone())
                .collect();
            (!names.is_empty()).then_some((app_id, names))
        })
        .collect()
}

/// Account details, with the public persona name.
pub async fn account(client: &SteamClient) -> AccountData {
    let mut data = client.get_account_data().await;
    data.persona_name = client.own_persona_name().await;
    data
}

/// A Steam library folder that can be installed into.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LibraryFolder {
    pub path: PathBuf,
    /// Free space on its drive, when known.
    pub free_bytes: Option<u64>,
}

/// The library folders games can be installed into: the configured
/// `steam_library` first, then every other one that has a `steamapps`
/// directory.
pub async fn folders() -> Vec<LibraryFolder> {
    let configured = crate::core::config::load_launcher_config()
        .await
        .ok()
        .map(|config| std::path::PathBuf::from(config.steam_library_path))
        .filter(|path| !path.as_os_str().is_empty());
    install_targets(configured, crate::library::all_library_roots().await)
        .into_iter()
        .map(|path| LibraryFolder {
            free_bytes: crate::core::utils::available_space_for(&path),
            path,
        })
        .collect()
}

/// `configured` first, even with nothing installed in it yet (an install
/// creates its `steamapps/`), then each of `roots` that has a `steamapps/`.
fn install_targets(
    configured: Option<std::path::PathBuf>,
    roots: Vec<std::path::PathBuf>,
) -> Vec<std::path::PathBuf> {
    // `roots` are canonical, so compare against the canonical configured path.
    let configured = configured.map(|path| std::fs::canonicalize(&path).unwrap_or(path));
    let mut targets: Vec<_> = configured.iter().cloned().collect();
    targets.extend(
        roots
            .into_iter()
            .filter(|root| root.join("steamapps").is_dir() && Some(root) != configured.as_ref()),
    );
    targets
}

#[cfg(test)]
mod tests;
