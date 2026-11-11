//! App metadata: store records with a short-TTL cache, DLC lists, depots,
//! branches, achievements, launch options, DRM checks and artwork.

use crate::core::config::{info_cache_ttl, load_info_cache, save_info_cache};
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::DlcState;
use crate::steam_client::{EncryptedTicketOutcome, SteamClient, StoreAppInfo};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Price region used when none is given.
pub const DEFAULT_COUNTRY: &str = "US";

/// Resolve the store country: `requested` (normalized) or [`DEFAULT_COUNTRY`].
pub fn resolve_country(requested: Option<&str>) -> Result<String> {
    let Some(raw) = requested else {
        return Ok(DEFAULT_COUNTRY.to_string());
    };
    crate::core::utils::normalize_country(raw).ok_or_else(|| {
        TypedError::new(
            ErrorKind::InvalidInput,
            format!("invalid country code `{raw}`; use a two-letter ISO code like US or DE"),
        )
        .into()
    })
}

/// `(dlc app id, store name)` pairs sorted by id; `None` when the store has no
/// name for it.
pub type DlcNames = Vec<(u32, Option<String>)>;

/// Extra metadata: appinfo fields plus web-sourced system requirements.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ExtendedInfo {
    pub genres: Vec<String>,
    pub categories: Vec<String>,
    pub metacritic: Option<i64>,
    pub metacritic_url: Option<String>,
    pub website: Option<String>,
    pub requirements_minimum: Vec<String>,
    pub requirements_recommended: Vec<String>,
}

/// Everything known about one app.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub details: StoreAppInfo,
    pub dlc: DlcNames,
    /// Present when extended data was requested.
    pub extended: Option<ExtendedInfo>,
}

/// What [`app_info`] fetches.
#[derive(Debug, Clone)]
pub struct InfoRequest {
    pub app_ids: Vec<u32>,
    /// Also fetch [`ExtendedInfo`].
    pub extended: bool,
    /// Skip the metadata cache and fetch fresh.
    pub no_cache: bool,
    /// Store country (price region and cache key).
    pub country: String,
}

/// Store metadata for the requested apps, in request order.
///
/// Records are served from a short-TTL disk cache when possible; every miss is
/// then fetched over a single Steam logon with one batched store call.
/// `connect` is only called when something has to be fetched. With one app
/// an unknown id is a `NotFound` error; in a batch it is skipped.
pub async fn app_info(
    request: &InfoRequest,
    connect: impl AsyncFnOnce() -> Result<SteamClient>,
) -> Result<Vec<AppInfo>> {
    let InfoRequest {
        app_ids,
        extended,
        no_cache,
        country,
    } = request;
    let ttl = if *no_cache {
        std::time::Duration::ZERO
    } else {
        info_cache_ttl()
    };
    let single = app_ids.len() == 1;

    let mut base: HashMap<u32, (StoreAppInfo, DlcNames)> = HashMap::new();
    let mut misses: Vec<u32> = Vec::new();
    for &id in app_ids {
        match load_info_cache(id, country, ttl).await {
            Some(cached) => {
                base.insert(id, (cached.details, cached.dlc));
            }
            None => misses.push(id),
        }
    }

    let client = if !misses.is_empty() || *extended {
        Some(connect().await?)
    } else {
        None
    };

    if let Some(client) = client.as_ref().filter(|_| !misses.is_empty()) {
        let store = client
            .fetch_store_apps(&misses, country)
            .await
            .context("failed to fetch store information")?;
        for &id in &misses {
            let Some(details) = store.iter().find(|i| i.app_id == id).cloned() else {
                if single {
                    return Err(no_store_info(id));
                }
                tracing::warn!("no store information available for app {id}; skipping");
                continue;
            };
            // The DLC id list lives in appinfo (`common.dlc`), not the store record.
            let dlc_ids = client
                .get_extended_app_info(id)
                .await
                .map(|e| e.dlcs)
                .unwrap_or_default();
            let dlc = dlc_names(client, &dlc_ids, country).await;
            if let Err(e) = save_info_cache(id, country, &details, &dlc).await {
                tracing::warn!("could not cache info for app {id}: {e:#}");
            }
            base.insert(id, (details, dlc));
        }
    }

    let mut extended_by_id: HashMap<u32, ExtendedInfo> = HashMap::new();
    if let Some(client) = client.as_ref().filter(|_| *extended) {
        let ids: Vec<u32> = app_ids
            .iter()
            .copied()
            .filter(|id| base.contains_key(id))
            .collect();
        extended_by_id = extended_info(client, &ids, country).await;
    }

    let infos: Vec<AppInfo> = app_ids
        .iter()
        .filter_map(|id| {
            base.remove(id).map(|(details, dlc)| AppInfo {
                details,
                dlc,
                extended: extended_by_id.remove(id),
            })
        })
        .collect();
    if single && infos.is_empty() {
        return Err(no_store_info(app_ids[0]));
    }
    Ok(infos)
}

/// [`ExtendedInfo`] for each app; failures of the parts are logged and left
/// empty.
async fn extended_info(
    client: &SteamClient,
    app_ids: &[u32],
    country: &str,
) -> HashMap<u32, ExtendedInfo> {
    let category_names = client.store_category_names().await.unwrap_or_else(|e| {
        tracing::warn!("category names unavailable: {e:#}");
        Default::default()
    });
    let http = crate::core::net::steam_web_client().ok();
    let mut out = HashMap::new();
    for &id in app_ids {
        let mut ext = ExtendedInfo::default();
        match client.get_extended_app_info(id).await {
            Ok(pics) => {
                ext.metacritic = pics.metacritic_score;
                ext.metacritic_url = pics.metacritic_url;
                ext.website = pics.homepage;
                ext.genres = pics
                    .genre_ids
                    .iter()
                    .filter_map(|&g| crate::steam_client::genres_table::genre_name(g))
                    .map(String::from)
                    .collect();
                // Some ids share a name (two "DualShock Controller
                // Support" categories): list each name once.
                let mut seen = std::collections::HashSet::new();
                ext.categories = pics
                    .category_ids
                    .iter()
                    .filter_map(|c| category_names.get(c).cloned())
                    .filter(|name| seen.insert(name.clone()))
                    .collect();
            }
            Err(e) => tracing::warn!("appinfo unavailable for app {id}: {e:#}"),
        }
        if let Some(http) = &http {
            match crate::web::store::fetch_requirements(http, id, country).await {
                Ok(Some(req)) => {
                    ext.requirements_minimum = req.minimum;
                    ext.requirements_recommended = req.recommended;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("requirements unavailable for app {id}: {e:#}"),
            }
        }
        out.insert(id, ext);
    }
    out
}

fn no_store_info(app_id: u32) -> anyhow::Error {
    TypedError::new(
        ErrorKind::NotFound,
        format!("no store information available for app {app_id}"),
    )
    .into()
}

/// Store names for DLC ids in one batched call, sorted by id.
pub async fn dlc_names(client: &SteamClient, dlc_ids: &[u32], country: &str) -> DlcNames {
    let records = dlc_store_info(client, dlc_ids, country).await;
    names_from_records(dlc_ids, &records)
}

/// `(id, name)` pairs for `ids`, sorted by id, named from `records`.
fn names_from_records(ids: &[u32], records: &HashMap<u32, StoreAppInfo>) -> DlcNames {
    let mut names: DlcNames = ids
        .iter()
        .map(|&id| {
            let name = records.get(&id).map(|i| i.name.clone());
            (id, name.filter(|s| !s.is_empty()))
        })
        .collect();
    names.sort_by_key(|(id, _)| *id);
    names
}

/// Store records for DLC ids, keyed by id. Best-effort: a failed fetch logs
/// and yields an empty map.
pub async fn dlc_store_info(
    client: &SteamClient,
    dlc_ids: &[u32],
    country: &str,
) -> HashMap<u32, StoreAppInfo> {
    if dlc_ids.is_empty() {
        return HashMap::new();
    }
    client
        .fetch_store_apps(dlc_ids, country)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("could not fetch DLC store records: {e:#}");
            Vec::new()
        })
        .into_iter()
        .map(|i| (i.app_id, i))
        .collect()
}

/// One DLC of a base game.
#[derive(Debug, Clone)]
pub struct DlcEntry {
    pub app_id: u32,
    pub name: Option<String>,
    /// Ownership and local state; `None` when Steam did not report it.
    pub state: Option<DlcState>,
    /// The store record (price, release date, reviews), when available.
    pub store: Option<StoreAppInfo>,
}

/// The DLC of `app_id` with ownership, local state and store records,
/// sorted by id.
pub async fn dlc_list(client: &SteamClient, app_id: u32) -> Result<Vec<DlcEntry>> {
    let dlc_ids: Vec<u32> = client
        .get_extended_app_info(app_id)
        .await
        .map(|e| e.dlcs)
        .unwrap_or_default();
    let mut store = dlc_store_info(client, &dlc_ids, DEFAULT_COUNTRY).await;
    let names = names_from_records(&dlc_ids, &store);
    let states = client
        .dlc_states(app_id, &dlc_ids)
        .await
        .with_context(|| format!("failed to resolve DLC status for app {app_id}"))?;
    let mut state_by_id: HashMap<u32, DlcState> =
        states.into_iter().map(|s| (s.app_id, s)).collect();
    Ok(names
        .into_iter()
        .map(|(id, name)| DlcEntry {
            app_id: id,
            name,
            state: state_by_id.remove(&id),
            store: store.remove(&id),
        })
        .collect())
}

/// Enable or disable a DLC. Returns the base game's id. Steam must be
/// stopped for the change to stick (see [`crate::ops::steam`]).
pub async fn set_dlc(client: &SteamClient, dlc_id: u32, enable: bool) -> Result<u32> {
    client
        .set_dlc_enabled(dlc_id, enable)
        .await
        .with_context(|| {
            let verb = if enable { "enable" } else { "disable" };
            format!("failed to {verb} DLC {dlc_id}")
        })
}

/// The branches (betas) Steam reports for an app.
#[cfg(feature = "versioning")]
pub async fn branches(client: &SteamClient, app_id: u32) -> Result<Vec<String>> {
    client
        .fetch_branches(app_id)
        .await
        .with_context(|| format!("failed to fetch branches for app {app_id}"))
}

/// Switch an app to `branch`; the next update applies it.
#[cfg(feature = "versioning")]
pub async fn set_branch(client: &SteamClient, app_id: u32, branch: &str) -> Result<()> {
    client
        .update_app_branch(app_id, branch)
        .await
        .with_context(|| format!("failed to switch app {app_id} to branch {branch}"))
}

/// The account's achievements for an app.
pub async fn achievements(
    client: &SteamClient,
    app_id: u32,
) -> Result<Vec<crate::steam_client::GameAchievement>> {
    client
        .fetch_achievements(app_id)
        .await
        .with_context(|| format!("failed to fetch achievements for app {app_id}"))
}

/// The depots of an app.
pub async fn depots(
    client: &SteamClient,
    app_id: u32,
) -> Result<Vec<crate::steam_client::DepotInfo>> {
    client
        .get_depot_list(app_id)
        .await
        .with_context(|| format!("failed to load depots for app {app_id}"))
}

/// The launch options of an app.
pub async fn launch_options(
    client: &SteamClient,
    app_id: u32,
) -> Result<Vec<crate::steam_client::LaunchOptionInfo>> {
    client
        .fetch_launch_options(app_id)
        .await
        .with_context(|| format!("failed to load launch options for app {app_id}"))
}

/// The raw appinfo `common`/`extended` sections, for debugging.
pub async fn appinfo_debug(client: &SteamClient, app_id: u32) -> Result<String> {
    client.appinfo_debug(app_id).await
}

/// What Steam's DRM tickets say about an app.
#[derive(Debug)]
pub struct DrmReport {
    /// Store name, when known.
    pub name: Option<String>,
    /// Size of the ownership ticket; an error means no license.
    pub ownership: Result<usize>,
    /// The encrypted app ticket request.
    pub encrypted: Result<EncryptedTicketOutcome>,
}

/// Request the ownership and encrypted app tickets for an app.
pub async fn drm_check(client: &SteamClient, app_id: u32) -> DrmReport {
    let name = client
        .fetch_store_apps(&[app_id], DEFAULT_COUNTRY)
        .await
        .ok()
        .and_then(|apps| apps.into_iter().find(|a| a.app_id == app_id))
        .map(|a| a.name)
        .filter(|n| !n.is_empty());
    DrmReport {
        name,
        ownership: client.get_app_ticket(app_id).await.map(|t| t.len()),
        encrypted: client.request_encrypted_app_ticket(app_id).await,
    }
}

/// Steam CDN artwork variants for an app, in order of preference.
fn artwork_candidates(app_id: u32) -> [String; 3] {
    [
        format!("https://cdn.akamai.steamstatic.com/steam/apps/{app_id}/library_600x900_2x.jpg"),
        format!("https://cdn.akamai.steamstatic.com/steam/apps/{app_id}/header.jpg"),
        format!("https://steamcdn-a.akamaihd.net/steam/apps/{app_id}/library_capsule_2x.jpg"),
    ]
}

/// Library artwork for an app, downloaded into the image cache unless it is
/// already there (or `force`). With `output`, the image is copied there too.
/// Returns the path of the image.
pub async fn artwork(app_id: u32, output: Option<&Path>, force: bool) -> Result<PathBuf> {
    let cache_dir = crate::core::config::image_cache_dir()?;
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .with_context(|| format!("failed creating {}", cache_dir.display()))?;
    let cache_path = cache_dir.join(format!("{app_id}_library.jpg"));

    if force || tokio::fs::metadata(&cache_path).await.is_err() {
        let mut downloaded = false;
        gaki_steam_vent::install_crypto_provider();
        for url in artwork_candidates(app_id) {
            match reqwest::get(&url).await {
                Ok(resp) if resp.status().is_success() => {
                    let bytes = resp.bytes().await.context("failed reading image bytes")?;
                    tokio::fs::write(&cache_path, &bytes)
                        .await
                        .with_context(|| format!("failed writing {}", cache_path.display()))?;
                    downloaded = true;
                    break;
                }
                _ => continue,
            }
        }
        if !downloaded {
            bail!("no artwork found on the Steam CDN for app {app_id}");
        }
    }

    match output {
        Some(out) => {
            tokio::fs::copy(&cache_path, out)
                .await
                .with_context(|| format!("failed writing {}", out.display()))?;
            Ok(out.to_path_buf())
        }
        None => Ok(cache_path),
    }
}

/// A store record with its media, from the info cache when it has them.
pub async fn record_with_media(
    app_id: u32,
    connect: impl AsyncFnOnce() -> Result<SteamClient>,
) -> Result<StoreAppInfo> {
    use crate::core::config::{info_cache_ttl, load_info_cache};
    if let Some(cached) = load_info_cache(app_id, DEFAULT_COUNTRY, info_cache_ttl()).await
        // Older cache entries were written without media.
        && (!cached.details.screenshots.is_empty() || !cached.details.tags.is_empty())
    {
        return Ok(cached.details);
    }
    let client = connect().await?;
    client
        .fetch_store_apps(&[app_id], DEFAULT_COUNTRY)
        .await?
        .into_iter()
        .find(|a| a.app_id == app_id)
        .ok_or_else(|| no_store_info(app_id))
}

/// One asset URL of an app, optionally probed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Asset {
    /// `header`, `capsule`, `hero`, `background`, `logo`, `screenshot`,
    /// `trailer_thumb` or `trailer`.
    pub kind: &'static str,
    pub url: String,
    /// HTTP status of a HEAD request; `None` when not probed or unreachable.
    pub status: Option<u16>,
}

/// Every asset URL of a store record.
pub fn assets(info: &StoreAppInfo) -> Vec<Asset> {
    let mut entries: Vec<(&'static str, String)> = Vec::new();
    let a = &info.assets;
    for (kind, url) in [
        ("header", &a.header),
        ("capsule", &a.capsule),
        ("hero", &a.hero),
        ("background", &a.background),
        ("logo", &a.logo),
    ] {
        if let Some(u) = url {
            entries.push((kind, u.clone()));
        }
    }
    entries.extend(info.screenshots.iter().map(|u| ("screenshot", u.clone())));
    for t in &info.trailers {
        if let Some(u) = &t.thumbnail {
            entries.push(("trailer_thumb", u.clone()));
        }
        if let Some(u) = &t.url {
            entries.push(("trailer", u.clone()));
        }
    }
    entries
        .into_iter()
        .map(|(kind, url)| Asset {
            kind,
            url,
            status: None,
        })
        .collect()
}

/// HEAD each asset URL, eight at a time, filling in `status`.
pub async fn probe_assets(assets: &mut [Asset]) {
    let Ok(client) = crate::core::net::http_client(std::time::Duration::from_secs(15)) else {
        return;
    };
    let indices: Vec<usize> = (0..assets.len()).collect();
    for chunk in indices.chunks(8) {
        let mut set = tokio::task::JoinSet::new();
        for &i in chunk {
            let client = client.clone();
            let url = assets[i].url.clone();
            set.spawn(async move {
                let status = crate::core::net::send_with_retry(&client, client.head(&url))
                    .await
                    .ok()
                    .map(|r| r.status().as_u16());
                (i, status)
            });
        }
        while let Some(Ok((i, status))) = set.join_next().await {
            assets[i].status = status;
        }
    }
}

/// The artwork kinds [`save_artwork`] saves: the store and library images,
/// not screenshots or trailers.
pub const ARTWORK_KINDS: &[&str] = &["header", "capsule", "hero", "background", "logo"];

/// One image [`save_artwork`] tried to save.
#[derive(Debug)]
pub struct SavedArtwork {
    pub kind: &'static str,
    /// Where it was written, or why not.
    pub result: Result<PathBuf>,
}

/// The file an asset is saved as: `<app id>_<kind>.<ext>`, the extension
/// taken from the URL's path (`jpg` when it has none).
pub fn artwork_file_name(app_id: u32, kind: &str, url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| {
            !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .unwrap_or_else(|| "jpg".to_string());
    format!("{app_id}_{kind}.{ext}")
}

/// Download `info`'s [`ARTWORK_KINDS`] images into `dir` (created if
/// missing). An existing file is never overwritten: that image is reported
/// as an error and the others still save.
pub async fn save_artwork(info: &StoreAppInfo, dir: &Path) -> Result<Vec<SavedArtwork>> {
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("failed creating {}", dir.display()))?;
    let client = crate::core::net::http_client(std::time::Duration::from_secs(30))?;
    let mut saved = Vec::new();
    for asset in assets(info)
        .into_iter()
        .filter(|a| ARTWORK_KINDS.contains(&a.kind))
    {
        let path = dir.join(artwork_file_name(info.app_id, asset.kind, &asset.url));
        let result = save_one(&client, &asset.url, &path).await.map(|()| path);
        saved.push(SavedArtwork {
            kind: asset.kind,
            result,
        });
    }
    Ok(saved)
}

async fn save_one(client: &reqwest::Client, url: &str, path: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    if tokio::fs::try_exists(path).await.unwrap_or(false) {
        bail!("{} already exists", path.display());
    }
    let response = crate::core::net::send_with_retry(client, client.get(url)).await?;
    if !response.status().is_success() {
        bail!("{url}: HTTP {}", response.status());
    }
    let bytes = response
        .bytes()
        .await
        .context("failed reading image bytes")?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .with_context(|| format!("failed creating {}", path.display()))?;
    if let Err(e) = file.write_all(&bytes).await {
        drop(file);
        let _ = tokio::fs::remove_file(path).await;
        return Err(e).with_context(|| format!("failed writing {}", path.display()));
    }
    Ok(())
}

/// The store tag vocabulary, `(id, name)` sorted by id.
pub async fn tags(client: &SteamClient) -> Result<Vec<(u32, String)>> {
    client.fetch_tag_list().await
}

#[cfg(test)]
mod tests;
