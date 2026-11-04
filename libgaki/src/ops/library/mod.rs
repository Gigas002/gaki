//! The user's game library: owned, installed and Family-Shared apps merged
//! into one list.

use crate::core::config::load_library_cache;
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::LibraryGame;
use crate::library::{build_game_library, scan_installed_app_info};
use crate::steam_client::{SharedApp, SteamClient};
use anyhow::Result;

/// Build the merged owned + installed library.
///
/// Owned games come from Steam when `client` is logged in, falling back to the
/// on-disk cache, then to the official Steam client's local caches (which hold
/// the whole library on a machine where Steam is signed in).
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
    build_game_library(owned, installed, client.steam_id()).games
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

/// Best-effort game name from the offline library cache; `None` on a miss.
pub async fn cached_game_name(app_id: u32) -> Option<String> {
    load_library_cache().await.ok().and_then(|games| {
        games
            .into_iter()
            .find(|g| g.app_id == app_id)
            .map(|g| g.name)
    })
}

#[cfg(test)]
mod tests;
