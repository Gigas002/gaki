use crate::auth::{delete_session, load_session, save_session};
use crate::core::config::library_cache_path;
#[cfg(feature = "library")]
use crate::core::config::{load_launcher_config, load_library_cache, save_library_cache};
use crate::core::models::{DepotPlatform, QrEvent, SessionState, SteamGuardReq};
#[cfg(feature = "library")]
use crate::core::models::{DlcState, LibraryGame, OwnedGame};
#[cfg(feature = "install")]
use crate::core::models::{DownloadProgress, DownloadProgressState, ManifestSelection};
use crate::core::utils::extract_quoted_values;
#[cfg(feature = "cloud")]
use crate::library::cloud_sync::{CloudClient, CloudPathResolver, UfsSaveSpec, default_cloud_root};
#[cfg(feature = "install")]
use crate::library::depot_browser::{self, ManifestFileEntry};
use crate::web::cm_list::get_cm_endpoints;
use anyhow::{Context, Result, anyhow, bail};
#[cfg(feature = "library")]
use serde::Deserialize;
use std::collections::HashMap;
#[cfg(feature = "library")]
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;
#[cfg(feature = "library")]
use std::path::PathBuf;
#[cfg(feature = "library")]
use std::process::Command;
#[cfg(feature = "library")]
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gaki_steam_vent::auth::{
    AuthConfirmationHandler, ClientInfo, ConfirmationMethod, DeviceConfirmationHandler,
    FileGuardDataStore, RefreshToken, UserProvidedAuthConfirmationHandler,
};
use gaki_steam_vent::connection::Connection;
use gaki_steam_vent::{ConnectionError, ConnectionTrait, LoginError, ServerList};
use gaki_steam_vent_proto::steammessages_clientserver::{
    CMsgClientGetAppOwnershipTicket, CMsgClientRequestEncryptedAppTicket,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_clientserver_2::{
    CMsgClientGetDepotDecryptionKey, CMsgClientGetDepotDecryptionKeyResponse,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_clientserver_appinfo::{
    CMsgClientPICSProductInfoRequest, CMsgClientPICSProductInfoResponse,
    cmsg_client_picsproduct_info_request,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_clientserver_userstats::{
    CMsgClientGetUserStats, CMsgClientGetUserStatsResponse,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_contentsystem_steamclient::{
    CContentServerDirectory_GetCDNAuthToken_Request,
    CContentServerDirectory_GetCDNAuthToken_Response,
    CContentServerDirectory_GetManifestRequestCode_Request,
    CContentServerDirectory_GetManifestRequestCode_Response,
    CContentServerDirectory_GetServersForSteamPipe_Request,
    CContentServerDirectory_GetServersForSteamPipe_Response,
};
use gaki_steam_vent_proto::steammessages_familygroups_steamclient::{
    CFamilyGroups_GetFamilyGroupForUser_Request, CFamilyGroups_GetFamilyGroupForUser_Response,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_familygroups_steamclient::{
    CFamilyGroups_GetSharedLibraryApps_Request, CFamilyGroups_GetSharedLibraryApps_Response,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_player_steamclient::{
    CPlayer_GetGameAchievements_Request, CPlayer_GetGameAchievements_Response,
    CPlayer_GetOwnedGames_Request, CPlayer_GetOwnedGames_Response,
};
#[cfg(feature = "library")]
use gaki_steam_vent_proto::steammessages_storebrowse_steamclient::{
    CStoreBrowse_GetItems_Request, CStoreBrowse_GetItems_Response, EStoreAppType,
    StoreBrowseContext, StoreBrowseItemDataRequest, StoreItem, StoreItemID,
    store_item::trailers::Trailer,
};
use protobuf::Message;
#[cfg(feature = "library")]
use protobuf::MessageField;
use tokio::io::{AsyncWriteExt, duplex, sink};
#[cfg(feature = "install")]
use tokio::sync::mpsc::Receiver;

/// How long to wait for a single Steam CM connection/refresh-token logon before
/// giving up on that server. The initial WebSocket handshake has no timeout of
/// its own, so an unresponsive CM would otherwise block the command forever.
const CM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Upper bound on a single liveness probe ([`SteamClient::probe_alive`]). Generous
/// enough not to false-positive when the connection is merely busy (e.g. during a
/// heavy download), short enough to detect a dead socket within one probe cycle.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// How many CM servers to try before failing. `ServerList::pick_ws` rotates
/// round-robin, so each attempt hits a different server.
const CM_CONNECT_ATTEMPTS: usize = 3;

/// The device name gaki logs on as. Steam shows it in the account's list of
/// authorized devices (Steam Guard), so the user can tell which program holds
/// a session and revoke it.
pub(crate) const DEVICE_NAME: &str = "gaki";

/// What gaki tells Steam about itself on a password logon: [`DEVICE_NAME`],
/// with steam-vent's defaults for the rest (the Steam Client platform type,
/// which a CM logon needs).
pub(crate) fn client_info() -> ClientInfo {
    ClientInfo::new(DEVICE_NAME.to_string())
}

/// Where Steam Guard machine tokens live: `<cache dir>/machine_tokens.json`.
///
/// steam-vent used to write them to `$XDG_CACHE_HOME/steam-vent/`; a file
/// left there is moved over once, so the device stays remembered.
pub(crate) fn guard_data_path(cache_dir: &std::path::Path) -> std::path::PathBuf {
    let path = cache_dir.join("machine_tokens.json");
    let legacy = cache_dir
        .parent()
        .map(|base| base.join("steam-vent").join("machine_tokens.json"));
    if let Some(legacy) = legacy.filter(|legacy| !path.exists() && legacy.is_file()) {
        let moved = std::fs::create_dir_all(cache_dir).and_then(|()| std::fs::copy(&legacy, &path));
        match moved {
            Ok(_) => {
                let _ = std::fs::remove_file(&legacy);
                tracing::info!(from = %legacy.display(), to = %path.display(), "moved Steam Guard machine tokens");
            }
            Err(e) => tracing::warn!(error = %e, "could not move Steam Guard machine tokens"),
        }
    }
    path
}

/// The logon ID this process sends with every CM logon.
///
/// Steam ends an older session when a new logon arrives with the same
/// account and the same ID. A fixed ID (steam-vent sends 0 by default) makes
/// the TUI, the daemon and one-off commands kick each other off; a random
/// nonzero ID per process lets them stay online side by side, as several
/// Steam clients can.
pub(crate) fn logon_id() -> u32 {
    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| rand::random::<u32>().max(1))
}

/// Authenticate with a stored refresh token, with a per-attempt timeout and
/// automatic failover to the next CM server.
///
/// The refresh-token logon connects to a single WebSocket CM server but never
/// times out — a stalled CM leaves the whole command hanging. Each call
/// re-picks a server round-robin, so on timeout/error we simply retry,
/// advancing to the next one.
async fn access_with_retry(server_list: &ServerList, refresh_token: &str) -> Result<Connection> {
    let token = RefreshToken::new(refresh_token.to_string())
        .context("the saved Steam session is not a valid refresh token; run `gaki-cli login`")?;
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 1..=CM_CONNECT_ATTEMPTS {
        tracing::debug!(
            attempt,
            attempts = CM_CONNECT_ATTEMPTS,
            "Authenticating with stored refresh token ..."
        );
        match tokio::time::timeout(
            CM_CONNECT_TIMEOUT,
            Connection::login_with_refresh_token_and_logon_id(server_list, &token, logon_id()),
        )
        .await
        {
            Ok(Ok(connection)) => {
                tracing::debug!("Refresh-token authentication succeeded");
                return Ok(connection);
            }
            // Retrying these only deepens a lockout.
            Ok(Err(
                err @ ConnectionError::LoginError(
                    LoginError::RateLimited
                    | LoginError::InvalidCredentials
                    | LoginError::UnavailableAccount
                    | LoginError::InvalidSteamId,
                ),
            )) => {
                tracing::warn!("CM logon rejected: {err}; not retrying");
                return Err(anyhow::Error::new(err).context("refresh token login failed"));
            }
            Ok(Err(err)) => {
                tracing::warn!("CM connect attempt {attempt}/{CM_CONNECT_ATTEMPTS} failed: {err}");
                last_err = Some(anyhow::Error::new(err));
            }
            Err(_) => {
                tracing::warn!(
                    "CM connect attempt {attempt}/{CM_CONNECT_ATTEMPTS} timed out after {}s; \
                     trying next server",
                    CM_CONNECT_TIMEOUT.as_secs()
                );
                last_err = Some(anyhow!(
                    "Steam CM connection timed out after {}s",
                    CM_CONNECT_TIMEOUT.as_secs()
                ));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("failed to connect to any Steam CM server")))
        .context("refresh token login failed")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginState {
    Connected,
    AwaitingCredentialSession,
    AwaitingGuardConfirmation,
    AwaitingPollResult,
    AwaitingAccessTokenLogon,
    Complete,
    Offline,
}

pub use crate::core::models::{LaunchInfo, LaunchTarget};

#[cfg(feature = "library")]
#[derive(Debug, Clone)]
pub struct RawLaunchOption {
    pub executable: String,
    pub arguments: String,
}

#[cfg(feature = "library")]
#[derive(Debug, Clone)]
pub struct ExtendedAppInfo {
    pub name: Option<String>,
    pub dlcs: Vec<u32>,
    pub depots: Vec<(u32, String)>,
    pub launch_options: Vec<RawLaunchOption>,
    pub active_branch: String,
    /// `common/metacritic_score`.
    pub metacritic_score: Option<i64>,
    /// `common/metacritic_fullurl`.
    pub metacritic_url: Option<String>,
    /// `extended/homepage`.
    pub homepage: Option<String>,
    /// `common/genres` values (store genre ids).
    pub genre_ids: Vec<u32>,
    /// `common/category` keys (`category_N`).
    pub category_ids: Vec<u32>,
}

/// Human-facing store metadata for an app, sourced from the `StoreBrowse.GetItems`
/// service method over the Steam CM connection (no HTTPS storefront API). This is
/// the protocol-native replacement for the fields gaki previously scraped from
/// `store.steampowered.com/api/appdetails`.
///
/// Note: a few storefront-only fields have no equivalent in this protocol and are
/// intentionally absent — system requirements, Metacritic score, and
/// name-resolved store tags/genres/categories (StoreBrowse returns only numeric
/// ids for those, which would need a separate localized tag dictionary).
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoreAppInfo {
    pub app_id: u32,
    pub name: String,
    pub app_type: String,
    pub is_free: bool,
    pub is_early_access: bool,
    pub short_description: String,
    pub full_description: String,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub franchises: Vec<String>,
    pub release_date: Option<String>,
    pub coming_soon: bool,
    pub price: Option<String>,
    pub discount_pct: i32,
    pub platforms: Vec<String>,
    pub review_summary: Option<String>,
    /// Artwork URLs (header/cover/hero/background/logo).
    pub assets: StoreAppAssets,
    /// Country the prices were quoted for.
    #[serde(default)]
    pub country: String,
    /// Final price in minor units.
    #[serde(default)]
    pub price_cents: Option<i64>,
    /// Pre-discount price, formatted.
    #[serde(default)]
    pub original_price: Option<String>,
    /// Pre-discount price in minor units.
    #[serde(default)]
    pub original_price_cents: Option<i64>,
    /// Unix time the active discount ends.
    #[serde(default)]
    pub discount_end: Option<u64>,
    /// Store refuses sale in this country.
    #[serde(default)]
    pub region_locked: bool,
    /// Every package and bundle the store offers.
    #[serde(default)]
    pub purchase_options: Vec<StorePurchaseOption>,
    /// Store tags, heaviest first.
    #[serde(default)]
    pub tags: Vec<StoreTag>,
    /// Age rating from the store, if any.
    #[serde(default)]
    pub rating: Option<StoreRating>,
    /// Official social links.
    #[serde(default)]
    pub links: Vec<StoreLink>,
    /// Screenshot URLs, all-ages first.
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// Trailers, highlights first.
    #[serde(default)]
    pub trailers: Vec<StoreTrailer>,
}

/// A store tag with its per-app weight.
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoreTag {
    pub id: u32,
    pub name: Option<String>,
    pub weight: u32,
}

/// Age rating (e.g. PEGI, ESRB).
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoreRating {
    pub system: String,
    pub rating: String,
    pub descriptors: Vec<String>,
    pub required_age: i32,
}

/// An official link (youtube, discord, …).
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoreLink {
    pub kind: String,
    pub url: String,
}

/// A store trailer. `url` is an HLS/DASH stream, or the short mp4 microtrailer.
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoreTrailer {
    pub name: String,
    pub movie_id: Option<u64>,
    pub url: Option<String>,
    pub thumbnail: Option<String>,
}

/// A package or bundle for an app.
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StorePurchaseOption {
    /// `package` or `bundle`.
    pub kind: String,
    pub id: u32,
    pub name: String,
    pub price: Option<String>,
    pub price_cents: Option<i64>,
    pub original_price: Option<String>,
    pub discount_pct: i32,
    /// Extra saving from bundling, if a bundle.
    pub bundle_discount_pct: i32,
    pub included_games: u32,
    pub discount_end: Option<u64>,
}

#[cfg(feature = "library")]
fn purchase_option_summary(
    p: &gaki_steam_vent_proto::steammessages_storebrowse_steamclient::store_item::PurchaseOption,
) -> StorePurchaseOption {
    let (kind, id) = if p.bundleid() > 0 {
        ("bundle", p.bundleid())
    } else {
        ("package", p.packageid())
    };
    let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_string());
    StorePurchaseOption {
        kind: kind.to_string(),
        id: id.max(0) as u32,
        name: p.purchase_option_name().to_string(),
        price: nonempty(p.formatted_final_price()),
        price_cents: (p.final_price_in_cents() > 0).then(|| p.final_price_in_cents()),
        original_price: nonempty(p.formatted_original_price())
            .filter(|_| p.original_price_in_cents() > p.final_price_in_cents()),
        discount_pct: p.discount_pct(),
        bundle_discount_pct: p.bundle_discount_pct(),
        included_games: p.included_game_count().max(0) as u32,
        discount_end: p
            .active_discounts
            .iter()
            .map(|d| d.discount_end_date())
            .max()
            .filter(|&t| t > 0)
            .map(u64::from),
    }
}

/// Artwork URLs for a store app. Built from the StoreBrowse `assets` block when
/// present, falling back to Steam's conventional per-appid CDN paths so a caller
/// (e.g. Heroic) always has usable URLs instead of guessing them itself.
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoreAppAssets {
    /// Wide store header / capsule (`header.jpg`).
    pub header: Option<String>,
    /// Portrait library cover (`library_600x900`).
    pub capsule: Option<String>,
    /// Wide library hero background.
    pub hero: Option<String>,
    /// Store-page background.
    pub background: Option<String>,
    /// Transparent game logo (`logo.png`). Not in StoreBrowse — always the
    /// conventional CDN URL.
    pub logo: Option<String>,
}

#[cfg(feature = "library")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepotInfo {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    pub config: String,
    pub is_owned: Option<bool>,
}

/// One depot's current manifest on one branch, as advertised by PICS appinfo.
/// Steam only exposes the *current* id per branch — historical (older) ids are
/// not enumerable here and must be looked up on SteamDB. Emitted by
/// [`SteamClient::list_depot_manifests`] (the `gaki-cli manifests` command).
#[cfg(feature = "library")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepotManifestInfo {
    pub depot_id: u32,
    pub depot_name: Option<String>,
    pub branch: String,
    pub manifest_id: u64,
    pub size: u64,
}

/// A pre-install size estimate for a game on a given platform, derived purely
/// from PICS appinfo (no manifest/CDN fetch). `download_size` is the compressed
/// bytes transferred; `disk_size` is the installed (uncompressed) footprint.
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default)]
pub struct InstallSizeEstimate {
    pub download_size: u64,
    pub disk_size: u64,
    pub depot_count: u64,
}

/// A single Steam achievement for a game, combining the game's achievement
/// definition (name/description/icons/global rarity) with the logged-in user's
/// unlock state. Shaped to match how launchers (Heroic/Legendary) report
/// achievements.
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default)]
pub struct GameAchievement {
    /// Stable API/internal name (e.g. `ACH_WIN_ONE_GAME`).
    pub api_name: String,
    /// Localized display name.
    pub name: String,
    /// Localized description (may be empty for hidden achievements).
    pub description: String,
    /// Whether the achievement is hidden until unlocked.
    pub hidden: bool,
    /// Full CDN URL of the unlocked (color) icon.
    pub icon_unlocked: String,
    /// Full CDN URL of the locked (gray) icon.
    pub icon_locked: String,
    /// Global unlock rate across all players, as a percentage (rarity).
    pub global_percent: f32,
    /// Whether the logged-in user has unlocked it.
    pub unlocked: bool,
    /// Unix time (seconds) the user unlocked it, if unlocked.
    pub unlock_time: Option<u32>,
}

/// One entry from a game's PICS `config/launch` table — a way Steam knows to
/// start the game (an executable, its arguments, and platform constraints).
#[cfg(feature = "library")]
#[derive(Debug, Clone, Default)]
pub struct LaunchOptionInfo {
    /// The launch entry's key (`"0"`, `"1"`, …); `"0"` is the default.
    pub id: String,
    pub description: String,
    pub executable: String,
    pub arguments: String,
    pub working_dir: String,
    /// Target OS (`windows`/`linux`/`macos`), or empty for any.
    pub oslist: String,
    /// Target architecture (`32`/`64`), or empty for any.
    pub osarch: String,
    /// Steam launch `type` (e.g. `default`, `option1`), if set.
    pub launch_type: String,
}

#[derive(Debug, Clone)]
pub struct ConfirmationPrompt {
    pub requirement: SteamGuardReq,
    pub details: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AccountData {
    pub steam_id: u64,
    pub account_name: String,
    /// Public persona
    pub persona_name: Option<String>,
    pub country: String,      // GeoIP Country
    pub authed_machines: u32, // Steam Guard count
    pub flags: u32,           // Account Flags
    pub email: String,
    pub email_validated: bool,
    pub vac_bans: u32, // Num VAC bans
    pub vac_banned_apps: Vec<u32>,
}

/// Encrypted app ticket request outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptedTicketOutcome {
    /// Serialized `EncryptedAppTicket` protobuf message.
    Issued(Vec<u8>),
    /// Steam declined; carries the eresult.
    Refused(i32),
}

/// A game available to the account through Steam Family Sharing.
#[cfg(feature = "library")]
#[derive(Debug, Clone)]
pub struct SharedApp {
    pub app_id: u32,
    pub name: String,
    /// SteamID64 of the family member who owns (lends) this app, if reported.
    pub owner_steamid: Option<u64>,
}

#[derive(Clone)]
pub struct SteamClient {
    connection: Option<Connection>,
    state: LoginState,
    connected_at: Option<Instant>,
    active_cm: Option<SocketAddr>,
    server_list: Option<ServerList>,
    pending_confirmations: Vec<ConfirmationPrompt>,
    /// Why the last session restore failed.
    restore_error: Option<Arc<anyhow::Error>>,
}

// SteamClient methods are implemented across these submodules, each behind
// the feature it serves.
mod client;
pub mod ident;
mod persona;
mod process;

#[cfg(feature = "library")]
mod categories;
#[cfg(feature = "library")]
mod content;
#[cfg(feature = "library")]
pub use content::ContentServer;
#[cfg(feature = "library")]
pub mod genres_table;
#[cfg(feature = "library")]
mod launch;
#[cfg(feature = "library")]
mod library;
#[cfg(feature = "library")]
mod manifests;
#[cfg(feature = "library")]
mod tags;
#[cfg(feature = "library")]
pub mod tags_table;
#[cfg(feature = "library")]
pub use launch::{LaunchTargetChoice, launch_targets, select_launch_entry};

#[cfg(feature = "install")]
mod install;
#[cfg(feature = "install")]
mod manage;
#[cfg(feature = "install")]
mod shaders;
#[cfg(feature = "install")]
pub use install::InstallOptions;
#[cfg(feature = "install")]
pub use shaders::ShaderBucket;

#[cfg(feature = "collections")]
mod cloudconfig;
#[cfg(feature = "collections")]
mod collections;
#[cfg(feature = "collections")]
pub use collections::RemoteNamespace;

#[cfg(feature = "workshop")]
mod workshop;
#[cfg(feature = "workshop")]
mod workshop_manifest;

#[cfg(feature = "social")]
mod chat;
#[cfg(feature = "social")]
mod friends;
#[cfg(feature = "social")]
mod profile;
#[cfg(feature = "social")]
pub use chat::{ChatMessage, SentMessage};
#[cfg(feature = "social")]
pub use friends::{AddedFriend, Friend, Roster};
#[cfg(feature = "social")]
pub use profile::UserProfile;

#[cfg(feature = "inventory")]
mod market;
#[cfg(feature = "inventory")]
pub use market::{
    InventoryItem, MarketPrice, MarketSearchResult, MyMarketState, WalletBalance, inventory_via,
    market_price, market_search, my_listings_via, wallet_via,
};

#[cfg(feature = "store")]
mod events;
#[cfg(feature = "store")]
mod players;
#[cfg(feature = "store")]
mod storequery;
#[cfg(feature = "store")]
mod wishlist;
#[cfg(feature = "store")]
pub use events::StoreEvent;
#[cfg(feature = "store")]
pub use storequery::{DealsScope, QueryPage};
#[cfg(feature = "store")]
pub use wishlist::WishlistEntry;

/// Terminate the process tree rooted at `pid`. With `force`, kill immediately;
/// otherwise ask it to exit first (SIGTERM) and only SIGKILL the stragglers after
/// a short grace period so the game can shut down cleanly.
fn kill_process_tree(pid: u32, force: bool) {
    use rustix::process::{Pid, Signal, kill_process};
    // `Pid::from_raw` refuses 0 and negatives, which `kill` would read as a
    // whole process group or every process.
    let tree: Vec<Pid> = process_tree_pids(pid as i32)
        .into_iter()
        .filter_map(Pid::from_raw)
        .collect();
    let signal_all = |signal| {
        for &p in &tree {
            // Gone already, or not ours to signal: nothing to do.
            let _ = kill_process(p, signal);
        }
    };
    if !force {
        signal_all(Signal::TERM);
        std::thread::sleep(Duration::from_millis(500));
    }
    signal_all(Signal::KILL);
}

fn process_tree_pids(root: i32) -> Vec<i32> {
    let mut parent_of: Vec<(i32, i32)> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for entry in dir.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<i32>().ok()) else {
                continue;
            };
            let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
                continue;
            };
            if let Some(ppid) = status
                .lines()
                .find_map(|l| l.strip_prefix("PPid:"))
                .and_then(|v| v.trim().parse::<i32>().ok())
            {
                parent_of.push((pid, ppid));
            }
        }
    }
    let mut tree = vec![root];
    let mut i = 0;
    while i < tree.len() {
        let cur = tree[i];
        for &(pid, ppid) in &parent_of {
            if ppid == cur && !tree.contains(&pid) {
                tree.push(pid);
            }
        }
        i += 1;
    }
    tree
}

pub fn sanitize_install_dir(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let sanitized = sanitized.trim().to_string();
    // Dot-only or empty names escape common/.
    if crate::core::utils::is_safe_dir_name(&sanitized) {
        sanitized
    } else {
        "_".to_string()
    }
}

/// Collect the DLC appids whose content is recorded as installed in an appmanifest.
/// Steam tags each DLC depot under `InstalledDepots` with a `"dlcappid" "<id>"` line.
#[cfg(feature = "library")]
fn parse_installed_dlc_appids(content: &str) -> HashSet<u32> {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#""dlcappid"\s*"(\d+)""#).unwrap());
    RE.captures_iter(content)
        .filter_map(|caps| caps[1].parse::<u32>().ok())
        .collect()
}

/// Collect the DLC appids listed in an appmanifest's `DisabledDLC` value(s).
/// The value is a comma-separated list of appids; multiple blocks may each carry one.
#[cfg(feature = "library")]
fn parse_disabled_dlc_appids(content: &str) -> HashSet<u32> {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#""DisabledDLC"\s*"([^"]*)""#).unwrap());
    RE.captures_iter(content)
        .flat_map(|caps| {
            caps[1]
                .split(',')
                .filter_map(|s| s.trim().parse::<u32>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Add or remove a DLC appid from every `DisabledDLC` list in an appmanifest's text.
/// When `disabled` is true and no `DisabledDLC` key exists, one is inserted into the
/// `UserConfig` and `MountedConfig` blocks.
#[cfg(feature = "library")]
fn apply_dlc_disabled(content: &str, dlc_appid: u32, disabled: bool) -> String {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#""DisabledDLC"(\s*)"([^"]*)""#).unwrap());
    let dlc = dlc_appid.to_string();

    if RE.is_match(content) {
        return RE
            .replace_all(content, |caps: &regex::Captures| {
                let mut list: Vec<String> = caps[2]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && *s != dlc)
                    .collect();
                if disabled {
                    list.push(dlc.clone());
                }
                format!("\"DisabledDLC\"{}\"{}\"", &caps[1], list.join(","))
            })
            .into_owned();
    }

    if disabled {
        // No DisabledDLC key yet — add one to the per-user config blocks.
        let mut out = content.to_string();
        for block in ["MountedConfig", "UserConfig"] {
            if let Some(pos) = out.find(&format!("\"{block}\""))
                && let Some(rel) = out[pos..].find('{')
            {
                let at = pos + rel + 1;
                out.insert_str(at, &format!("\n\t\t\"DisabledDLC\"\t\t\"{dlc}\""));
            }
        }
        return out;
    }

    content.to_string()
}

/// Collect the non-empty display names from a list of `CreatorHomeLink`s
/// (developers / publishers / franchises in a `StoreItem.basic_info`).
#[cfg(feature = "library")]
fn creator_names(
    creators: &[gaki_steam_vent_proto::steammessages_storebrowse_steamclient::store_item::basic_info::CreatorHomeLink],
) -> Vec<String> {
    creators
        .iter()
        .map(|c| c.name().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Map a `StoreBrowse` `StoreItem` protobuf into our display-oriented
/// [`StoreAppInfo`]. Defensive throughout: every nested message is optional, so
/// missing data yields empty fields rather than failing.
#[cfg(feature = "library")]
fn store_item_to_app_info(item: &StoreItem, country: &str) -> StoreAppInfo {
    let basic = item.basic_info.as_ref();

    let release = item.release.as_ref();
    let release_date = release.and_then(|r| {
        let custom = r.custom_release_date_message();
        if !custom.is_empty() {
            Some(custom.to_string())
        } else if r.steam_release_date() > 0 {
            Some(unix_to_ymd(r.steam_release_date() as i64))
        } else {
            None
        }
    });

    let platforms = item
        .platforms
        .as_ref()
        .map(|p| {
            let mut v = Vec::new();
            if p.windows() {
                v.push("Windows".to_string());
            }
            if p.mac() {
                v.push("macOS".to_string());
            }
            if p.steamos_linux() {
                v.push("Linux".to_string());
            }
            v
        })
        .unwrap_or_default();

    let option = item.best_purchase_option.as_ref();
    let (price, discount_pct) = match option {
        _ if item.is_free() => (Some("Free".to_string()), 0),
        Some(p) if !p.formatted_final_price().is_empty() => (
            Some(p.formatted_final_price().to_string()),
            p.discount_pct(),
        ),
        _ => (None, 0),
    };
    let priced = option.filter(|_| !item.is_free() && price.is_some());
    let price_cents = priced.map(|p| p.final_price_in_cents());
    let original_price_cents = priced
        .map(|p| p.original_price_in_cents())
        .filter(|&c| c > 0 && Some(c) != price_cents);
    let original_price = priced
        .map(|p| p.formatted_original_price().to_string())
        .filter(|s| !s.is_empty() && original_price_cents.is_some());
    let discount_end = priced
        .and_then(|p| {
            p.active_discounts
                .iter()
                .map(|d| d.discount_end_date())
                .max()
        })
        .filter(|&t| t > 0)
        .map(u64::from);

    let review_summary = item
        .reviews
        .as_ref()
        .and_then(|r| r.summary_filtered.as_ref())
        .and_then(|s| {
            let label = s.review_score_label();
            if label.is_empty() {
                None
            } else if s.review_count() == 0 {
                // "No user reviews": the numbers would only repeat it.
                Some(label.to_string())
            } else {
                Some(format!(
                    "{} ({}% positive, {} reviews)",
                    label,
                    s.percent_positive(),
                    s.review_count()
                ))
            }
        });

    StoreAppInfo {
        app_id: item.appid(),
        name: item.name().to_string(),
        app_type: store_app_type_label(item.type_()),
        is_free: item.is_free(),
        is_early_access: item.is_early_access(),
        short_description: basic
            .map(|b| b.short_description().to_string())
            .unwrap_or_default(),
        full_description: crate::web::store::strip_html(
            crate::web::store::strip_bbcode(item.full_description_bbcode()).trim(),
        ),
        developers: basic
            .map(|b| creator_names(&b.developers))
            .unwrap_or_default(),
        publishers: basic
            .map(|b| creator_names(&b.publishers))
            .unwrap_or_default(),
        franchises: basic
            .map(|b| creator_names(&b.franchises))
            .unwrap_or_default(),
        release_date,
        coming_soon: release.map(|r| r.is_coming_soon()).unwrap_or(false),
        price,
        discount_pct,
        platforms,
        review_summary,
        assets: store_assets(item),
        country: country.to_string(),
        price_cents,
        original_price,
        original_price_cents,
        discount_end,
        region_locked: item.unvailable_for_country_restriction(),
        purchase_options: item
            .purchase_options
            .iter()
            .map(purchase_option_summary)
            .collect(),
        tags: store_tags(item),
        rating: store_rating(item),
        links: store_links(item),
        screenshots: store_screenshots(item),
        trailers: store_trailers(item),
    }
}

/// Tags sorted by weight, names unresolved.
#[cfg(feature = "library")]
fn store_tags(item: &StoreItem) -> Vec<StoreTag> {
    let mut tags: Vec<StoreTag> = item
        .tags
        .iter()
        .filter(|t| t.tagid() != 0)
        .map(|t| StoreTag {
            id: t.tagid(),
            name: None,
            weight: t.weight(),
        })
        .collect();
    tags.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.id.cmp(&b.id)));
    tags
}

#[cfg(feature = "library")]
fn store_rating(item: &StoreItem) -> Option<StoreRating> {
    let r = item.game_rating.as_ref()?;
    if r.rating().is_empty() && r.type_().is_empty() {
        return None;
    }
    // Board name parsed from the image path.
    let system = if r.type_().is_empty() {
        r.image_url()
            .split("game_ratings/")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("")
            .to_string()
    } else {
        r.type_().to_string()
    };
    Some(StoreRating {
        system,
        rating: r.rating().to_string(),
        descriptors: r.descriptors.clone(),
        required_age: r.required_age(),
    })
}

#[cfg(feature = "library")]
fn store_links(item: &StoreItem) -> Vec<StoreLink> {
    use gaki_steam_vent_proto::steammessages_storebrowse_steamclient::EStoreLinkType as L;
    item.links
        .iter()
        .filter(|l| !l.url().is_empty())
        .map(|l| {
            let kind = match l.link_type.and_then(|t| t.enum_value().ok()) {
                Some(L::k_EStoreLinkType_YouTube) => "youtube",
                Some(L::k_EStoreLinkType_Facebook) => "facebook",
                Some(L::k_EStoreLinkType_Twitter) => "twitter",
                Some(L::k_EStoreLinkType_Twitch) => "twitch",
                Some(L::k_EStoreLinkType_Discord) => "discord",
                _ => "other",
            };
            StoreLink {
                kind: kind.to_string(),
                url: l.url().to_string(),
            }
        })
        .collect()
}

/// Expand `${FILENAME}` in a StoreBrowse URL template.
#[cfg(feature = "library")]
fn asset_url(fmt: &str, filename: &str) -> Option<String> {
    if fmt.is_empty() || filename.is_empty() {
        return None;
    }
    let path = fmt.replace("${FILENAME}", filename);
    Some(if path.starts_with("http") {
        path
    } else {
        format!("https://shared.cloudflare.steamstatic.com/store_item_assets/{path}")
    })
}

#[cfg(feature = "library")]
fn store_screenshots(item: &StoreItem) -> Vec<String> {
    let fmt = item
        .assets
        .as_ref()
        .map(|a| a.asset_url_format())
        .unwrap_or("");
    let Some(shots) = item.screenshots.as_ref() else {
        return Vec::new();
    };
    shots
        .all_ages_screenshots
        .iter()
        .chain(shots.mature_content_screenshots.iter())
        .filter_map(|s| screenshot_url(fmt, s.filename()))
        .collect()
}

/// Screenshot filenames may already be full paths.
#[cfg(feature = "library")]
fn screenshot_url(fmt: &str, filename: &str) -> Option<String> {
    if filename.starts_with("steam/") {
        return Some(format!(
            "https://shared.cloudflare.steamstatic.com/store_item_assets/{filename}"
        ));
    }
    asset_url(fmt, filename)
}

#[cfg(feature = "library")]
fn store_trailers(item: &StoreItem) -> Vec<StoreTrailer> {
    let Some(trailers) = item.trailers.as_ref() else {
        return Vec::new();
    };
    trailers
        .highlights
        .iter()
        .chain(trailers.other_trailers.iter())
        .map(|t| {
            // Thumbnail path is `<movie id>/<hash>/movie_600x337.jpg`.
            let movie_id = t
                .screenshot_medium()
                .split('/')
                .next()
                .and_then(|s| s.parse::<u64>().ok());
            StoreTrailer {
                name: t.trailer_name().to_string(),
                movie_id,
                url: trailer_video_url(t),
                thumbnail: asset_url(t.trailer_url_format(), t.screenshot_medium()),
            }
        })
        .collect()
}

/// Steam's video CDN. Trailer paths (`adaptive_trailers[].cdn_path`,
/// `microtrailer[].filename`) are relative to it, not to `trailer_url_format`.
#[cfg(feature = "library")]
const TRAILER_CDN: &str = "https://video.fastly.steamstatic.com/store_trailers/";

/// A trailer's playable URL: the HLS H.264 stream (mpv, browsers), then DASH
/// H.264, then any other adaptive stream, then the short mp4 microtrailer.
/// The store stopped sending plain mp4 renditions (`trailer_max`/`_480p`).
#[cfg(feature = "library")]
fn trailer_video_url(t: &Trailer) -> Option<String> {
    let adaptive = |encoding: &str| {
        t.adaptive_trailers
            .iter()
            .find(|a| a.encoding() == encoding)
            .map(|a| a.cdn_path())
    };
    adaptive("hls_h264")
        .or_else(|| adaptive("dash_h264"))
        .or_else(|| t.adaptive_trailers.first().map(|a| a.cdn_path()))
        .or_else(|| {
            t.microtrailer
                .iter()
                .find(|m| m.type_() == "video/mp4")
                .map(|m| m.filename())
        })
        .filter(|path| !path.is_empty())
        .map(|path| format!("{TRAILER_CDN}{path}"))
}

/// Resolve a `StoreItem`'s artwork URLs. Each is built from the StoreBrowse
/// `assets` block (`asset_url_format` with the asset filename substituted for
/// `${FILENAME}`) when available, otherwise from Steam's conventional per-appid
/// CDN paths so the caller always gets a usable URL.
#[cfg(feature = "library")]
fn store_assets(item: &StoreItem) -> StoreAppAssets {
    let appid = item.appid();
    let assets = item.assets.as_ref();
    let fmt = assets.map(|a| a.asset_url_format()).unwrap_or("");

    let from_fmt = |filename: &str| -> Option<String> {
        if filename.is_empty() || fmt.is_empty() {
            return None;
        }
        let path = fmt.replace("${FILENAME}", filename);
        if path.starts_with("http") {
            Some(path)
        } else {
            Some(format!(
                "https://shared.cloudflare.steamstatic.com/store_item_assets/{path}"
            ))
        }
    };
    let cdn =
        |file: &str| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}");
    let pick = |native: Option<String>, fallback: &str| native.or_else(|| Some(cdn(fallback)));

    StoreAppAssets {
        header: pick(assets.and_then(|a| from_fmt(a.header())), "header.jpg"),
        capsule: pick(
            assets.and_then(|a| from_fmt(a.library_capsule_2x())),
            "library_600x900_2x.jpg",
        ),
        hero: pick(
            assets.and_then(|a| from_fmt(a.library_hero_2x())),
            "library_hero.jpg",
        ),
        background: pick(
            assets.and_then(|a| from_fmt(a.page_background())),
            "page_bg_generated_v6b.jpg",
        ),
        // StoreBrowse has no logo field; use the conventional CDN URL.
        logo: Some(cdn("logo.png")),
    }
}

/// Human-readable label for a `StoreBrowse` app type.
#[cfg(feature = "library")]
fn store_app_type_label(t: EStoreAppType) -> String {
    match t {
        EStoreAppType::k_EStoreAppType_Game => "Game",
        EStoreAppType::k_EStoreAppType_Demo => "Demo",
        EStoreAppType::k_EStoreAppType_Mod => "Mod",
        EStoreAppType::k_EStoreAppType_Movie => "Movie",
        EStoreAppType::k_EStoreAppType_DLC => "DLC",
        EStoreAppType::k_EStoreAppType_Guide => "Guide",
        EStoreAppType::k_EStoreAppType_Software => "Software",
        EStoreAppType::k_EStoreAppType_Video => "Video",
        EStoreAppType::k_EStoreAppType_Series => "Series",
        EStoreAppType::k_EStoreAppType_Episode => "Episode",
        EStoreAppType::k_EStoreAppType_Hardware => "Hardware",
        EStoreAppType::k_EStoreAppType_Music => "Soundtrack",
        EStoreAppType::k_EStoreAppType_Beta => "Beta",
        EStoreAppType::k_EStoreAppType_Tool => "Tool",
        EStoreAppType::k_EStoreAppType_Advertising => "Advertising",
    }
    .to_string()
}

/// Convert a Unix timestamp (seconds, UTC) to a `YYYY-MM-DD` date string using
/// Howard Hinnant's days-from-civil algorithm. Avoids pulling in a date crate.
pub fn unix_to_ymd(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    // Shift epoch to 0000-03-01 so leap days fall at the end of the era.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Steam store category IDs that denote *online* multiplayer (as opposed to
/// local/LAN play). A title carrying any of these effectively needs a network
/// connection for its core multiplayer experience.
/// 20 = MMO, 36 = Online PvP, 38 = Online Co-op.
const ONLINE_MULTIPLAYER_CATEGORIES: &[u32] = &[20, 36, 38];
/// Steam store category ID for Single-player.
const SINGLE_PLAYER_CATEGORY: u32 = 2;

/// Infer whether a game requires an online connection from its PICS `common`
/// store-category map (keyed `category_<id>`).
///
/// Steam has no dedicated "online required" field, so this is a heuristic: a
/// title is treated as online-required when it advertises an online-multiplayer
/// category (MMO / Online PvP / Online Co-op) but does *not* advertise
/// single-player support — i.e. there is no documented way to play it offline.
pub fn category_online_required(categories: &HashMap<String, String>) -> bool {
    let has = |id: u32| {
        categories
            .get(&format!("category_{id}"))
            .map(|v| v != "0")
            .unwrap_or(false)
    };
    let online = ONLINE_MULTIPLAYER_CATEGORIES.iter().any(|&id| has(id));
    online && !has(SINGLE_PLAYER_CATEGORY)
}

/// Best-effort: relocate app `appid`'s entry from `from_lib` to `to_lib` in
/// `libraryfolders.vdf` (or just add it when `from_lib == to_lib`, e.g. for an
/// import). Logs and continues on any problem — Steam reconciles the index from
/// the appmanifests on its next launch.
#[cfg(feature = "install")]
async fn update_libraryfolders_for(from_lib: &Path, to_lib: &Path, appid: u32, install_dir: &Path) {
    // Steam doesn't know libraries in gaki's own layout, on purpose.
    use crate::library::layout::Layout;
    if Layout::of(from_lib).is_flat() || Layout::of(to_lib).is_flat() {
        return;
    }
    let roots = crate::library::all_library_roots().await;
    let Some(vdf_path) = crate::library::relocate::find_libraryfolders_vdf(&roots) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&vdf_path) else {
        tracing::warn!("could not read libraryfolders.vdf");
        return;
    };
    let size = crate::library::relocate::dir_size(install_dir);
    match crate::library::relocate::update_libraryfolders_apps(&text, appid, from_lib, to_lib, size)
    {
        Some(updated) => {
            if let Err(e) = std::fs::write(&vdf_path, updated) {
                tracing::warn!("could not write libraryfolders.vdf: {e}");
            }
        }
        None => tracing::warn!(
            "could not locate the library entry in libraryfolders.vdf; Steam will reconcile on next launch"
        ),
    }
}

/// Build the full Steam Community CDN URL for an achievement icon hash. Returns
/// it unchanged if it's already a URL, and empty for an empty hash.
#[cfg(feature = "library")]
fn achievement_icon_url(appid: u32, icon: &str) -> String {
    if icon.is_empty() {
        String::new()
    } else if icon.starts_with("http") {
        icon.to_string()
    } else {
        format!(
            "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{appid}/{icon}"
        )
    }
}

/// Parse the binary-KV achievement schema from a `ClientGetUserStats` response
/// into a map of achievement api-name → `(stat_id, bit)`, so the user's unlock
/// blocks (keyed by stat id, indexed by bit) can be matched to each achievement.
///
/// Schema shape: `<appid> { stats { <statid> { type "4" bits { <bit> { name
/// "ACH_…" … } } } } }` — `type == 4` marks an achievement bitfield stat.
#[cfg(feature = "library")]
fn parse_achievement_schema(schema: &[u8]) -> HashMap<String, (u32, u32)> {
    if schema.is_empty() {
        return HashMap::new();
    }
    // The binary-KV parser recurses one frame per nesting level; a malformed or
    // misaligned blob can recurse on a run of zero bytes and overflow the stack.
    // Parse on a thread with a generous stack so it can never crash the process.
    let bytes = schema.to_vec();
    std::thread::Builder::new()
        .name("ach-schema-parse".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || parse_achievement_schema_inner(&bytes))
        .ok()
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

/// Inner schema parse. Parses the blob directly as binary KeyValues (no
/// appinfo-style offset scanning, which could feed the parser misaligned data).
#[cfg(feature = "library")]
fn parse_achievement_schema_inner(schema: &[u8]) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    let Ok(vdf) = steam_vdf_parser::parse_binary(schema) else {
        return map;
    };
    let Some(root) = vdf.as_obj() else {
        return map;
    };
    // `stats` is directly under the appid-keyed root (or one level deeper).
    let stats = root.get("stats").and_then(|v| v.as_obj()).or_else(|| {
        root.values()
            .next()
            .and_then(|v| v.as_obj())
            .and_then(|o| o.get("stats"))
            .and_then(|v| v.as_obj())
    });
    let Some(stats) = stats else {
        return map;
    };

    for (stat_key, stat_val) in stats.iter() {
        let Ok(stat_id) = stat_key.parse::<u32>() else {
            continue;
        };
        let Some(stat_obj) = stat_val.as_obj() else {
            continue;
        };
        // Achievement-bitfield stats are exactly those carrying a `bits` table.
        // (Don't filter on `type`: in binary KV it's an integer, not "4".)
        let Some(bits) = stat_obj.get("bits").and_then(|v| v.as_obj()) else {
            continue;
        };
        for (bit_key, bit_val) in bits.iter() {
            let Ok(bit) = bit_key.parse::<u32>() else {
                continue;
            };
            if let Some(name) = bit_val
                .as_obj()
                .and_then(|o| o.get("name"))
                .and_then(|v| v.as_str())
            {
                map.insert(name.to_string(), (stat_id, bit));
            }
        }
    }
    map
}

/// Platform-matched non-DLC depot rows.
#[cfg(feature = "library")]
pub(crate) fn platform_depot_rows<'a, 'text>(
    depots: &'a steam_vdf_parser::Obj<'text>,
    platform: DepotPlatform,
) -> Vec<(
    u32,
    &'a steam_vdf_parser::Obj<'text>,
    Option<&'a steam_vdf_parser::Obj<'text>>,
)> {
    let mut rows = Vec::new();
    for (key, value) in depots.iter() {
        // Only numeric keys are depots.
        let Ok(depot_id) = key.parse::<u32>() else {
            continue;
        };
        let Some(obj) = value.as_obj() else { continue };
        // Exclude DLC content depots.
        if obj.get("dlcappid").is_some() {
            continue;
        }
        let oslist = obj
            .get("config")
            .and_then(|v| v.as_obj())
            .and_then(|c| c.get("oslist"))
            .and_then(|v| v.as_str());
        if !should_keep_depot(oslist, platform) {
            continue;
        }
        let public = obj
            .get("manifests")
            .and_then(|v| v.as_obj())
            .and_then(|m| m.get("public"))
            .and_then(|v| v.as_obj());
        rows.push((depot_id, obj, public));
    }
    rows
}

pub fn should_keep_depot(oslist: Option<&str>, target: DepotPlatform) -> bool {
    match target {
        DepotPlatform::Windows => match oslist {
            Some(os) => {
                let os = os.to_lowercase();
                if os.contains("windows") {
                    return true;
                }
                if os.contains("linux") || os.contains("macos") {
                    return false;
                }
                true
            }
            None => true,
        },
        DepotPlatform::Linux => match oslist {
            Some(os) => {
                let os = os.to_lowercase();
                if os.contains("linux") {
                    return true;
                }
                if os.contains("windows") || os.contains("macos") {
                    return false;
                }
                true
            }
            None => true,
        },
    }
}

fn map_confirmation(method: &ConfirmationMethod) -> ConfirmationPrompt {
    let details = method.confirmation_details().to_string();
    let requirement = match method.confirmation_type() {
        "email" => SteamGuardReq::EmailCode {
            domain_hint: details.clone(),
        },
        "device code" => SteamGuardReq::DeviceCode,
        "device confirmation" => SteamGuardReq::DeviceConfirmation,
        _ => SteamGuardReq::DeviceConfirmation,
    };

    ConfirmationPrompt {
        requirement,
        details,
    }
}

#[cfg(feature = "library")]
#[derive(Debug, Deserialize)]
struct ProductInfoEnvelopeWrapper(pub HashMap<String, ProductInfoEnvelope>);

#[cfg(feature = "library")]
impl ProductInfoEnvelopeWrapper {
    pub fn into_inner(self) -> Option<ProductInfoEnvelope> {
        self.0.into_values().next()
    }
}

#[cfg(feature = "library")]
fn parse_product_info_envelope(vdf: &str) -> Result<ProductInfoEnvelope> {
    if let Ok(parsed) = keyvalues_serde::from_str::<ProductInfoEnvelope>(vdf) {
        return Ok(parsed);
    }
    let wrapper: ProductInfoEnvelopeWrapper =
        keyvalues_serde::from_str(vdf).context("failed parsing product info VDF (wrapper)")?;
    wrapper
        .into_inner()
        .context("product info envelope was empty")
}

#[cfg(feature = "library")]
fn parse_launch_info_from_vdf(appid: u32, raw_vdf: &str) -> Result<Vec<LaunchInfo>> {
    let parsed: ProductInfoEnvelope =
        parse_product_info_envelope(raw_vdf).context("failed to parse product info VDF")?;

    let config = parsed
        .appinfo
        .as_ref()
        .and_then(|appinfo| appinfo.config.as_ref())
        .or(parsed.config.as_ref())
        .ok_or_else(|| anyhow!("missing config section in product info for app {appid}"))?;

    if config.launch.is_empty() {
        bail!("no launch entries found for app {appid}")
    }

    let mut options = Vec::new();
    for (id, entry) in &config.launch {
        let exe = entry.executable.as_deref().unwrap_or("");
        let os_list = entry.config.as_ref().and_then(|c| c.oslist.as_deref());
        let description = entry.description.as_deref().unwrap_or("Game");

        // Pick the launch target from the entry's oslist, falling back to the
        // executable extension and finally the host OS.
        let target = if let Some(os) = os_list {
            if os.contains("linux") {
                LaunchTarget::NativeLinux
            } else if os.contains("windows") {
                LaunchTarget::WindowsProton
            } else if os.contains("macos") {
                continue; // we don't launch macOS builds
            } else {
                LaunchTarget::WindowsProton
            }
        } else if exe.ends_with(".exe") || exe.ends_with(".bat") {
            LaunchTarget::WindowsProton
        } else {
            LaunchTarget::NativeLinux
        };

        options.push(LaunchInfo {
            app_id: appid,
            id: id.clone(),
            description: if description == "Game" && !exe.is_empty() {
                exe.to_string()
            } else {
                description.to_string()
            },
            executable: exe.to_string(),
            arguments: entry.arguments.clone().unwrap_or_default(),
            workingdir: entry.workingdir.clone(),
            target,
        });
    }

    if options.is_empty() {
        bail!("no suitable launch option found for app {appid}");
    }

    // Sort options: prefer key "0", then by id
    options.sort_by(|a, b| {
        if a.id == "0" {
            return std::cmp::Ordering::Less;
        }
        if b.id == "0" {
            return std::cmp::Ordering::Greater;
        }
        a.id.cmp(&b.id)
    });

    Ok(options)
}

/// Whether the buffer is text VDF.
fn pics_buffer_is_text(buffer: &[u8]) -> bool {
    buffer
        .first()
        .map(|&b| b == 0x22 || b == 0x7B)
        .unwrap_or(false)
}

pub fn find_vdf_in_pics(buffer: &[u8]) -> Result<steam_vdf_parser::Vdf<'static>> {
    let is_text = pics_buffer_is_text(buffer);

    if is_text {
        let text = String::from_utf8_lossy(buffer);
        return steam_vdf_parser::parse_text(&text)
            .map(|v| v.into_owned())
            .map_err(|e| anyhow!("Text VDF parse error: {}", e));
    }

    if let Ok(vdf) = steam_vdf_parser::parse_binary(buffer) {
        return Ok(vdf.into_owned());
    }

    for offset in 1..std::cmp::min(128, buffer.len()) {
        if let Ok(vdf) = steam_vdf_parser::parse_binary(&buffer[offset..]) {
            tracing::info!("Success! Found VDF at offset {}", offset);
            return Ok(vdf.into_owned());
        }
    }

    bail!("Failed to locate valid VDF (Text or Binary) in PICS buffer")
}

/// PICS appinfo nests an app's sections (`common`, `extended`, `config`, `depots`)
/// under a single root key — either the literal `appinfo` or the numeric appid.
/// Return the inner value that actually holds those sections, descending one level
/// past the wrapper when present so callers can navigate by section name directly.
pub(crate) fn pics_app_section<'a, 'text>(
    root: &'a steam_vdf_parser::Value<'text>,
) -> &'a steam_vdf_parser::Value<'text> {
    fn has_sections(v: &steam_vdf_parser::Value) -> bool {
        ["common", "extended", "config", "depots"]
            .iter()
            .any(|k| v.get(k).is_some())
    }
    if has_sections(root) {
        return root;
    }
    if let Some(inner) = root
        .as_obj()
        .and_then(|o| o.values().find(|v| has_sections(v)))
    {
        return inner;
    }
    root
}

/// Returns whether the user has signalled an abort for the in-progress
/// download/verify. A poisoned lock is treated as "not aborted" so a transient
/// lock failure can't spuriously cancel the operation.
#[cfg(feature = "install")]
pub(crate) fn download_aborted(
    state: &Arc<std::sync::RwLock<crate::core::models::DownloadState>>,
) -> bool {
    state
        .read()
        .map(|s| s.abort_signal.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(false)
}

/// Send a terminal `Failed` progress message.
#[cfg(feature = "install")]
pub(crate) async fn emit_failed(
    tx: &tokio::sync::mpsc::Sender<DownloadProgress>,
    msg: impl Into<String>,
) {
    let _ = tx.send(DownloadProgress::failed(msg)).await;
}

/// Send `Failed`, keeping the error's kind.
#[cfg(feature = "install")]
pub(crate) async fn emit_failed_err(
    tx: &tokio::sync::mpsc::Sender<DownloadProgress>,
    context: &str,
    err: &anyhow::Error,
) {
    let _ = tx.send(DownloadProgress::failed_with(context, err)).await;
}

/// Send `Failed` as a user stop.
#[cfg(feature = "install")]
pub(crate) async fn emit_cancelled(tx: &tokio::sync::mpsc::Sender<DownloadProgress>) {
    let err = anyhow!(crate::core::error::TypedError::new(
        crate::core::error::ErrorKind::Cancelled,
        "stopped by request",
    ));
    let _ = tx
        .send(DownloadProgress::failed_with("download", &err))
        .await;
}

/// Options for [`run_depot_loop`].
#[cfg(feature = "install")]
pub(crate) struct DepotLoopOpts {
    /// Re-verify existing chunks instead of downloading.
    pub verify_mode: bool,
    /// Whole-app total; `0` accumulates per-depot.
    pub grand_total_bytes: u64,
    /// Depot-pinned manifests; failures become hard errors.
    pub manifest_overrides: Option<std::collections::HashMap<u32, u64>>,
}

/// Depot fetch loop; `None` reports failure.
/// Content servers; `None` after reporting failure.
#[cfg(feature = "install")]
pub(crate) async fn fetch_content_hosts(
    client: &SteamClient,
    connection: &Connection,
    tx: &tokio::sync::mpsc::Sender<DownloadProgress>,
    appid: u32,
) -> Option<Vec<ContentServer>> {
    tracing::info!("Fetching Content Servers for AppID: {}...", appid);
    match client.get_content_servers(connection.cell_id()).await {
        Ok(h) => Some(h),
        Err(e) => {
            emit_failed_err(tx, "Failed to fetch content servers", &e).await;
            None
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "download loop context; becomes a struct in the libgaki move (2.3)"
)]
#[cfg(feature = "install")]
pub(crate) async fn run_depot_loop(
    client: &SteamClient,
    connection: &Connection,
    tx: &tokio::sync::mpsc::Sender<DownloadProgress>,
    shared_state: &Arc<std::sync::RwLock<crate::core::models::DownloadState>>,
    appid: u32,
    selections: Vec<ManifestSelection>,
    dest: &Path,
    hosts: &[ContentServer],
    opts: &DepotLoopOpts,
) -> Option<Vec<(u32, u64, u64)>> {
    let verb = if opts.verify_mode {
        "Verifying"
    } else {
        "Downloading"
    };
    let fail_verb = if opts.verify_mode {
        "download/verify"
    } else {
        "download"
    };
    let mut successful_depots = Vec::new();
    for selection in selections {
        tracing::info!(
            "Starting download for Depot {} (GID: {})...",
            selection.depot_id,
            selection.manifest_id
        );
        // Reset the current-depot counters so per-depot progress restarts.
        if let Ok(mut state) = shared_state.write() {
            state.status_text = format!("{verb} depot {}", selection.depot_id);
            state.depot_id = selection.depot_id;
            state.depot_downloaded_bytes = 0;
            state.depot_total_bytes = 0;
        }

        // A downgrade names this depot's manifest explicitly, so a request
        // code that comes back 401/empty (typical for very old manifests) is a
        // hard, clearly-reported failure rather than a silent skip.
        let is_override = opts
            .manifest_overrides
            .as_ref()
            .is_some_and(|m| m.contains_key(&selection.depot_id));
        if is_override {
            let ok = matches!(
                client
                    .get_manifest_request_code(appid, selection.depot_id, selection.manifest_id)
                    .await,
                Ok(code) if code != 0
            );
            if !ok {
                emit_failed(
                    tx,
                    format!(
                        "Steam declined a request code for manifest {} on depot {} — it may be too old, or require owning the game with a non-anonymous login",
                        selection.manifest_id, selection.depot_id
                    ),
                )
                .await;
                return None;
            }
        }

        // Missing key skips; overridden depots hard-fail.
        let key = match client.get_depot_key(appid, selection.depot_id).await {
            Ok(k) => k,
            Err(e) => {
                if is_override {
                    emit_failed(
                        tx,
                        format!(
                            "No depot key for depot {} — downgrade requires owning the game with a non-anonymous login: {}",
                            selection.depot_id, e
                        ),
                    )
                    .await;
                    return None;
                }
                tracing::warn!(
                    "Skipping Depot {} (No Key/Not Owned): {}",
                    selection.depot_id,
                    e
                );
                continue;
            }
        };
        // A valid depot key is exactly 32 bytes; a short/all-zero key would
        // decrypt chunks to garbage (the chunk path then fails the zip parse
        // with "Could not find EOCD").
        tracing::debug!(
            "Depot {} key: {} bytes, all_zero={}",
            selection.depot_id,
            key.len(),
            key.iter().all(|&b| b == 0)
        );

        match client
            .download_depot_to_with_hosts(
                appid,
                selection.depot_id,
                selection.manifest_id,
                dest,
                shared_state.clone(),
                hosts,
                opts.grand_total_bytes,
                connection,
                key,
                opts.verify_mode,
            )
            .await
        {
            Ok(depot_size) => {
                successful_depots.push((selection.depot_id, selection.manifest_id, depot_size));
            }
            Err(e) => {
                if download_aborted(shared_state) {
                    // Else a stop looked like success.
                    emit_cancelled(tx).await;
                    return None;
                }
                let context = format!("Failed to {fail_verb} depot {}", selection.depot_id);
                emit_failed_err(tx, &context, &e).await;
                return None;
            }
        }
    }
    Some(successful_depots)
}

/// Send the initial `Queued` progress message.
#[cfg(feature = "install")]
pub(crate) async fn emit_queued(
    tx: &tokio::sync::mpsc::Sender<DownloadProgress>,
    msg: impl Into<String>,
) {
    let _ = tx.send(DownloadProgress::queued(msg)).await;
}

/// Send a terminal `Completed` progress message.
#[cfg(feature = "install")]
pub(crate) async fn emit_completed(
    tx: &tokio::sync::mpsc::Sender<DownloadProgress>,
    msg: impl Into<String>,
) {
    let _ = tx.send(DownloadProgress::completed(msg)).await;
}

/// Forward byte counters every 250ms.
#[cfg(feature = "install")]
pub(crate) fn spawn_progress_reporter(
    tx: tokio::sync::mpsc::Sender<DownloadProgress>,
    state: Arc<std::sync::RwLock<crate::core::models::DownloadState>>,
    report_state: DownloadProgressState,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            ticker.tick().await;
            let snapshot = match state.read() {
                Ok(s) => Some((
                    s.is_downloading,
                    s.downloaded_bytes,
                    s.reused_bytes,
                    s.total_bytes,
                    s.status_text.clone(),
                    s.depot_id,
                    s.depot_downloaded_bytes,
                    s.depot_total_bytes,
                )),
                Err(_) => None,
            };
            let Some((
                downloading,
                downloaded,
                reused,
                total,
                status,
                depot_id,
                depot_downloaded,
                depot_total,
            )) = snapshot
            else {
                break;
            };
            if !downloading {
                break;
            }
            // Stop if the receiver is gone (terminal message already consumed).
            if tx
                .send(DownloadProgress {
                    state: report_state,
                    bytes_downloaded: downloaded,
                    reused_bytes: reused,
                    total_bytes: total,
                    current_file: status,
                    depot_id,
                    depot_bytes_downloaded: depot_downloaded,
                    depot_total_bytes: depot_total,
                    error_kind: None,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

/// The `depots` object, descending wrappers.
#[cfg(feature = "library")]
pub(crate) fn pics_depots_value<'a, 'text>(
    vdf: &'a steam_vdf_parser::Vdf<'text>,
) -> Option<&'a steam_vdf_parser::Value<'text>> {
    pics_app_section(vdf.value()).get("depots")
}

/// Steam Auto-Cloud save-file rules from an app's `ufs/savefiles` PICS section.
/// Each entry names a root token, sub-path, filename glob, and recursion flag.
#[cfg(feature = "cloud")]
pub(crate) fn ufs_save_specs_from_section(section: &steam_vdf_parser::Value) -> Vec<UfsSaveSpec> {
    let mut specs = Vec::new();
    if let Some(savefiles) = section.get_obj(&["ufs", "savefiles"]) {
        for (_, entry) in savefiles.iter() {
            let get = |k: &str| entry.get(k).and_then(|v| v.as_str());
            let root = get("root").unwrap_or("").to_string();
            if root.is_empty() {
                continue;
            }
            specs.push(UfsSaveSpec {
                root,
                path: get("path").unwrap_or("").to_string(),
                pattern: get("pattern").unwrap_or("*").to_string(),
                recursive: get("recursive") == Some("1"),
            });
        }
    }
    specs
}

/// DLC app ids declared by an app's PICS section, read from the canonical
/// `extended/listofdlc` field (a comma-separated string of app ids). Order is
/// preserved and duplicates dropped.
#[cfg(feature = "library")]
pub(crate) fn dlc_ids_from_section(section: &steam_vdf_parser::Value) -> Vec<u32> {
    let mut dlcs: Vec<u32> = Vec::new();
    if let Some(list) = section.get_str(&["extended", "listofdlc"]) {
        for id in list.split(',').filter_map(|p| p.trim().parse::<u32>().ok()) {
            if !dlcs.contains(&id) {
                dlcs.push(id);
            }
        }
    }
    dlcs
}

pub fn parse_pics_product_info(buffer: &[u8]) -> Result<HashMap<u64, u64>> {
    let is_text = pics_buffer_is_text(buffer);

    if is_text {
        parse_text_vdf(buffer)
    } else {
        parse_binary_vdf_with_offset(buffer)
    }
}

/// Walk a parsed PICS appinfo VDF root and collect its `public`-branch depot →
/// manifest-id map. Looks for `depots` at the root or one level under `appinfo`,
/// skips non-English-tagged depots, and resolves each manifest id via
/// [`extract_manifest_id_robust`]. Shared by the text and binary parse paths.
fn depot_map_from_vdf_root(root: &steam_vdf_parser::Value) -> HashMap<u64, u64> {
    let mut depot_map = HashMap::new();
    let depots_val = pics_app_section(root).get("depots");

    if let Some(depots) = depots_val.and_then(|v| v.as_obj()) {
        for (key, value) in depots.iter() {
            if let Ok(depot_id) = key.parse::<u64>() {
                let lang = value
                    .get_obj(&["config"])
                    .and_then(|c| c.get("language"))
                    .and_then(|l| l.as_str());
                if let Some(lang) = lang
                    && lang != "english"
                    && !lang.is_empty()
                {
                    continue;
                }

                if let Some(m_id) = extract_manifest_id_robust(value, "public") {
                    depot_map.insert(depot_id, m_id);
                }
            }
        }
    }
    depot_map
}

fn parse_text_vdf(data: &[u8]) -> Result<HashMap<u64, u64>> {
    let text = String::from_utf8_lossy(data);
    let mut depot_map = HashMap::new();

    if let Ok(vdf) = steam_vdf_parser::parse_text(&text) {
        depot_map = depot_map_from_vdf_root(vdf.value());
    }

    if depot_map.is_empty() {
        let mut current_depot = 0;
        let mut inside_depots = false;
        let mut inside_manifests = false;
        let mut inside_public = false;
        let mut depot_langs = HashMap::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.contains("\"depots\"") {
                inside_depots = true;
                continue;
            }
            if !inside_depots {
                continue;
            }

            if trimmed == "}" {
                if inside_public {
                    inside_public = false;
                } else if inside_manifests {
                    inside_manifests = false;
                }
                continue;
            }

            if trimmed.starts_with("\"manifests\"") {
                inside_manifests = true;
                continue;
            }
            if inside_manifests && trimmed.starts_with("\"public\"") {
                inside_public = true;
                continue;
            }

            let parts = extract_quoted_values(trimmed);
            if parts.len() == 1 {
                if let Ok(id) = parts[0].parse::<u64>() {
                    current_depot = id;
                    inside_manifests = false;
                    inside_public = false;
                }
            } else if parts.len() >= 2 && current_depot > 0 {
                let key = parts[0].to_lowercase();
                if inside_public && key == "gid" {
                    if let Ok(gid) = parts[1].parse::<u64>()
                        && gid > 0
                    {
                        depot_map.insert(current_depot, gid);
                    }
                } else if key == "language" {
                    depot_langs.insert(current_depot, parts[1].to_lowercase());
                } else if !inside_manifests && (key == "manifest" || key == "gid") {
                    // Fallback for flat structure
                    if let Ok(gid) = parts[1].parse::<u64>()
                        && gid > 0
                    {
                        depot_map.insert(current_depot, gid);
                    }
                }
            }
        }

        // Apply Language Filter to manual scan results
        depot_map.retain(|id, _| {
            if let Some(lang) = depot_langs.get(id)
                && lang != "english"
                && !lang.is_empty()
            {
                return false;
            }
            true
        });
    }

    if depot_map.is_empty() {
        bail!("Text scan found no depots");
    }

    Ok(depot_map)
}

fn parse_binary_vdf_with_offset(data: &[u8]) -> Result<HashMap<u64, u64>> {
    if let Ok(vdf) = find_vdf_in_pics(data) {
        // Ensure the located VDF is an object before walking it, preserving the
        // original error when it is not.
        vdf.as_obj().context("root is not an object")?;
        let depot_map = depot_map_from_vdf_root(vdf.value());
        if !depot_map.is_empty() {
            return Ok(depot_map);
        }
    }
    bail!("Failed to locate valid Binary VDF in PICS buffer")
}

fn extract_manifest_id_robust(value: &steam_vdf_parser::Value, branch: &str) -> Option<u64> {
    if let Some(obj) = value.as_obj() {
        // Deep search for branch manifest
        if let Some(manifests) = obj.get("manifests").and_then(|v| v.as_obj())
            && let Some(branch_entry) = manifests.get(branch)
        {
            // It can be a direct string or a gid object
            if let Some(gid_str) = branch_entry.as_str()
                && let Ok(gid) = gid_str.parse::<u64>()
            {
                return Some(gid);
            }
            if let Some(gid_val) = branch_entry.as_u64() {
                return Some(gid_val);
            }
            if let Some(branch_obj) = branch_entry.as_obj()
                && let Some(gid) = branch_obj.get("gid")
            {
                if let Some(s) = gid.as_str() {
                    return s.parse().ok();
                }
                return gid.as_u64();
            }
        }

        // Direct gid
        if let Some(gid_entry) = obj.get("gid") {
            if let Some(gid_str) = gid_entry.as_str() {
                return gid_str.parse::<u64>().ok();
            }
            if let Some(gid_val) = gid_entry.as_u64() {
                return Some(gid_val);
            }
        }
    }

    None
}

#[cfg(feature = "library")]
#[derive(Debug, Deserialize)]
struct ProductInfoEnvelope {
    #[serde(default)]
    appinfo: Option<ProductInfoAppInfo>,
    #[serde(default)]
    config: Option<ProductInfoConfig>,
}

#[cfg(feature = "library")]
#[derive(Debug, Deserialize)]
struct ProductInfoAppInfo {
    #[serde(default)]
    config: Option<ProductInfoConfig>,
}

#[cfg(feature = "library")]
#[derive(Debug, Deserialize)]
struct ProductInfoConfig {
    #[serde(default)]
    launch: HashMap<String, ProductLaunchEntry>,
}

#[cfg(feature = "library")]
#[derive(Debug, Deserialize)]
struct ProductLaunchEntry {
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub workingdir: Option<String>,
    #[serde(default)]
    pub config: Option<ProductLaunchConfigInner>,
}

#[cfg(feature = "library")]
#[derive(Debug, Deserialize)]
struct ProductLaunchConfigInner {
    #[serde(default)]
    oslist: Option<String>,
}

#[cfg(feature = "library")]
fn parse_installdir_from_acf(raw: &str) -> Option<String> {
    crate::core::acf::parse_app_manifest(raw).install_dir
}

/// Read the `name` value from an appmanifest, if present and non-empty.
#[cfg(feature = "install")]
pub(crate) fn parse_name_from_acf(raw: &str) -> Option<String> {
    crate::core::acf::parse_app_manifest(raw).display_name()
}

/// Read `LastOwner` (SteamID64) from an appmanifest; `None` when absent or `0`.
#[cfg(all(test, feature = "library"))]
pub(crate) fn parse_last_owner_from_acf(raw: &str) -> Option<u64> {
    crate::core::acf::parse_app_manifest(raw).last_owner
}

/// Download StateFlags
#[cfg(feature = "library")]
pub(crate) fn manifest_is_fully_installed(raw: &str) -> bool {
    crate::core::acf::parse_app_manifest(raw).fully_installed()
}

/// Fetch `StateUpdateRequired` flag
#[cfg(all(test, feature = "library"))]
pub(crate) fn manifest_update_pending(raw: &str) -> bool {
    crate::core::acf::parse_app_manifest(raw).update_pending()
}

#[cfg(feature = "library")]
fn parse_installed_depots_from_acf(raw: &str) -> HashMap<u64, u64> {
    let mut manifests = HashMap::new();
    let mut in_installed_depots = false;
    let mut current_depot: Option<u64> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.contains("\"InstalledDepots\"") {
            in_installed_depots = true;
            continue;
        }

        if !in_installed_depots {
            continue;
        }

        if trimmed == "}" {
            if current_depot.is_some() {
                current_depot = None;
                continue;
            }
            break;
        }

        let quoted = extract_quoted_values(trimmed);
        if quoted.len() == 1 {
            if let Ok(depot_id) = u64::from_str(&quoted[0]) {
                current_depot = Some(depot_id);
            }
        } else if quoted.len() >= 2
            && quoted[0] == "manifest"
            && let (Some(depot), Ok(manifest)) = (current_depot, u64::from_str(&quoted[1]))
        {
            manifests.insert(depot, manifest);
        }
    }

    manifests
}

#[cfg(feature = "library")]
fn parse_active_branch_from_acf(raw: &str) -> String {
    crate::core::acf::parse_app_manifest(raw).active_branch
}

#[cfg(feature = "versioning")]
fn rewrite_app_branch(raw: &str, branch: &str) -> String {
    let mut out = Vec::new();
    let mut in_user_config = false;
    let mut branch_updated = false;

    for line in raw.lines() {
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("\"UserConfig\"") {
            in_user_config = true;
            out.push(line.to_string());
            continue;
        }

        if in_user_config && trimmed == "{" {
            out.push(line.to_string());
            continue;
        }

        if in_user_config && trimmed == "}" {
            if !branch_updated {
                out.push(format!("\t\t\"BetaKey\"\t\t\"{branch}\""));
            }
            in_user_config = false;
            out.push(line.to_string());
            continue;
        }

        if in_user_config {
            let quoted = extract_quoted_values(trimmed);
            if !quoted.is_empty() && quoted[0].eq_ignore_ascii_case("betakey") {
                let indent = line
                    .chars()
                    .take_while(|ch| ch.is_whitespace())
                    .collect::<String>();
                out.push(format!("{indent}\"BetaKey\"\t\t\"{branch}\""));
                branch_updated = true;
                continue;
            }
        }

        out.push(line.to_string());
    }

    // If UserConfig was never found, we might need to add it, but for simplicity
    // we assume it exists in a valid Steam manifest.

    out.join("\n")
}

#[cfg(all(test, feature = "library"))]
mod tests;

#[cfg(feature = "library")]
fn split_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(ToString::to_string).collect()
}

/// How long a PICS appinfo reply is reused. One operation (an install, an
/// update check) asks for the same app several times, for its name, DLC
/// parent, depots and build id; this collapses those into one request without
/// letting a long-lived daemon serve an old build id.
#[cfg(feature = "library")]
const PICS_REUSE: Duration = Duration::from_secs(10);

/// A PICS appinfo reply and when it arrived.
#[cfg(feature = "library")]
type PicsReply = (Instant, Arc<Vec<u8>>);

/// Recent PICS appinfo replies, by app id.
#[cfg(feature = "library")]
#[derive(Default)]
pub(crate) struct PicsCache(std::sync::Mutex<HashMap<u32, PicsReply>>);

#[cfg(feature = "library")]
impl PicsCache {
    pub(crate) fn get(&self, appid: u32, now: Instant) -> Option<Arc<Vec<u8>>> {
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.retain(|_, (at, _)| now.saturating_duration_since(*at) < PICS_REUSE);
        map.get(&appid).map(|(_, buffer)| Arc::clone(buffer))
    }

    pub(crate) fn put(&self, appid: u32, now: Instant, buffer: Arc<Vec<u8>>) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(appid, (now, buffer));
    }
}

#[cfg(feature = "library")]
fn pics_cache() -> &'static PicsCache {
    static CACHE: std::sync::OnceLock<PicsCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(PicsCache::default)
}

/// One-app PICS `ProductInfo` buffer, reused for [`PICS_REUSE`].
#[cfg(feature = "library")]
pub(crate) async fn pics_app_buffer(
    connection: &Connection,
    appid: u32,
    job_context: &'static str,
) -> Result<Vec<u8>> {
    if let Some(buffer) = pics_cache().get(appid, Instant::now()) {
        return Ok(buffer.as_ref().clone());
    }
    let mut request = CMsgClientPICSProductInfoRequest::new();
    request
        .apps
        .push(cmsg_client_picsproduct_info_request::AppInfo {
            appid: Some(appid),
            ..Default::default()
        });

    let response: CMsgClientPICSProductInfoResponse =
        connection.job(request).await.context(job_context)?;

    let app = response
        .apps
        .iter()
        .find(|entry| entry.appid() == appid)
        .ok_or_else(|| anyhow!("missing appinfo payload for app {appid}"))?;
    let buffer = app.buffer().to_vec();
    pics_cache().put(appid, Instant::now(), Arc::new(buffer.clone()));
    Ok(buffer)
}

impl SteamClient {
    /// Borrow the live CM connection.
    pub(crate) fn require_connection(&self) -> Result<&Connection> {
        self.connection.as_ref().ok_or_else(|| {
            crate::core::error::TypedError::new(
                crate::core::error::ErrorKind::AuthRequired,
                "not logged in — run `gaki-cli login` first",
            )
            .into()
        })
    }

    /// Cloned connection for spawned tasks.
    #[cfg(any(feature = "install", feature = "cloud", feature = "social"))]
    pub(crate) fn require_connection_owned(&self) -> Result<Connection> {
        self.require_connection().cloned()
    }

    /// [`pics_app_buffer`] against this client's live connection.
    #[cfg(feature = "library")]
    pub(crate) async fn pics_buffer(
        &self,
        appid: u32,
        job_context: &'static str,
    ) -> Result<Vec<u8>> {
        pics_app_buffer(self.require_connection()?, appid, job_context).await
    }
}
