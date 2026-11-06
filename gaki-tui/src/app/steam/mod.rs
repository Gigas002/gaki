//! The TUI's view of Steam: one function per operation, each running a
//! `libgaki::ops` call in-process on the shared worker
//! ([`crate::app::worker`]) and mapping the result into the plain structs the
//! views render.

#[cfg(feature = "versioning")]
use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use crate::app::game::status::GameStatus;
use crate::app::worker;
use crate::utils::error::TuiError;
use libgaki::ops::config::{Change, GameEntryChange};

/// Session health.
#[derive(Debug, Clone)]
pub struct SessionHealth {
    pub logged_in: bool,
    pub account: Option<String>,
    pub steam_id: Option<u64>,
    pub daemon: bool,
}

/// The logged-in Steam account.
#[derive(Debug, Clone)]
pub struct Account {
    pub steam_id: u64,
    pub account_name: String,
    /// Steam persona / display name (e.g. "Gabe"); `None` when it can't
    /// be resolved.
    pub persona_name: Option<String>,
    pub country: String,
    pub email: String,
    pub email_validated: bool,
    pub authed_machines: u32,
    pub vac_bans: u32,
}

impl Account {
    /// User-facing player name: the Steam persona / display name, falling back
    /// to the account (login) name when no persona is known.
    pub fn display_name(&self) -> String {
        self.persona_name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.account_name.clone())
    }
}

/// The launcher configuration: the human-relevant top-level scalars of
/// `LauncherConfig`. Empty settings are `None`.
#[derive(Debug, Clone, Default)]
pub struct LauncherSettings {
    /// The Steam library path games install into.
    pub steam_library_path: Option<String>,
    /// The default Proton/Wine runtime.
    pub proton_version: Option<String>,
    /// Friends/chat presence (`"online"` / `"offline"`).
    pub chat_presence: Option<String>,
    /// Whether Steam Cloud save sync is enabled.
    pub enable_cloud_sync: bool,
}

impl LauncherSettings {
    /// Whether the configured presence is online. Defaults to `false`
    /// (offline) when the field is missing or unrecognised.
    pub fn is_online(&self) -> bool {
        self.chat_presence.as_deref() == Some("online")
    }
}

/// The Steam Wallet balance.
#[derive(Debug, Clone, Default)]
pub struct Wallet {
    /// Balance in the currency's minor units (cents).
    pub balance_cents: i64,
    /// Steam currency id.
    pub currency: u32,
    /// Wallet country code.
    pub country: String,
    /// Human-readable balance (e.g. "12.34 EUR").
    pub formatted: String,
}

/// A library entry's artwork URLs.
#[derive(Debug, Clone, Default)]
pub struct ArtworkUrls {
    pub header: Option<String>,
    pub capsule: Option<String>,
    pub hero: Option<String>,
    pub logo: Option<String>,
}

/// One library entry: a `LibraryGame` plus its artwork and store URLs.
#[derive(Debug, Clone)]
pub struct LibraryGame {
    pub app_id: u32,
    pub name: String,
    pub is_installed: bool,
    pub install_path: Option<String>,
    pub update_available: bool,
    pub is_owned: bool,
    pub is_family_shared: bool,
    pub platform: Option<String>,
    pub active_branch: Option<String>,
    pub assets: Option<ArtworkUrls>,
    pub store_url: Option<String>,
}

/// One Steam Cloud file of a game.
#[derive(Debug, Clone, Default)]
pub struct CloudFile {
    pub filename: String,
    pub size: u64,
    pub timestamp: i64,
}

/// Store metadata; only the fields the TUI shows.
#[derive(Debug, Clone, Default)]
pub struct StoreInfo {
    pub app_id: u32,
    pub name: String,
    pub app_type: String,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub description: String,
    pub reviews: Option<String>,
}

/// One achievement, with the user's unlock state and its global rarity.
#[derive(Debug, Clone, Default)]
pub struct Achievement {
    pub name: String,
    pub description: String,
    /// Whether the logged-in user has unlocked this achievement.
    pub unlocked: bool,
    /// Whether the achievement is visible (false = hidden until unlocked).
    pub visible: bool,
    /// Global unlock rate across all players, as a percentage (rarity).
    pub rarity: f32,
}

/// One friend on the roster, with status and current game.
#[derive(Debug, Clone, Default)]
pub struct Friend {
    /// SteamID64 of the friend.
    pub steam_id: u64,
    /// Display (persona) name, if known.
    pub persona_name: Option<String>,
    /// Online status: 0 offline, 1 online, 2 busy, 3 away, 4 snooze,
    /// 5 looking-to-trade, 6 looking-to-play.
    pub persona_state: Option<u32>,
    /// App id of the game the friend is currently playing, if any.
    pub game_app_id: Option<u32>,
    /// Name of the game the friend is currently playing, if known.
    pub game_name: Option<String>,
}

impl Friend {
    /// Display name, falling back to the SteamID when no persona is known.
    pub fn display_name(&self) -> String {
        self.persona_name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.steam_id.to_string())
    }

    /// Whether the friend is currently online (any non-offline persona state).
    pub fn is_online(&self) -> bool {
        !matches!(self.persona_state, None | Some(0))
    }

    /// The game the friend is currently playing, if any (name, else `app <id>`).
    pub fn current_game(&self) -> Option<String> {
        match (&self.game_name, self.game_app_id) {
            (Some(g), _) if !g.is_empty() => Some(g.clone()),
            (_, Some(id)) => Some(format!("app {}", id)),
            _ => None,
        }
    }
}

/// The account a friend search resolved to.
#[derive(Debug, Clone, Default)]
pub struct FriendLookup {
    /// Resolved SteamID64.
    pub steam_id: u64,
    /// Display (persona) name, if known.
    pub persona_name: Option<String>,
    /// The account's community profile URL, if known.
    pub profile_url: Option<String>,
}

impl FriendLookup {
    /// Display name, falling back to the SteamID when no persona is known.
    pub fn display_name(&self) -> String {
        self.persona_name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.steam_id.to_string())
    }
}

/// One chat message with a friend.
#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    /// SteamID64 of the sender.
    pub sender: u64,
    /// Whether the logged-in user sent this message.
    pub from_self: bool,
    /// Message body.
    pub message: String,
    /// Unix timestamp (seconds) the message was sent.
    pub timestamp: i64,
}

/// One inventory item stack.
#[derive(Debug, Clone, Default)]
pub struct InventoryItem {
    pub name: String,
    pub market_hash_name: String,
    /// Item type/category text (e.g. "Trading Card").
    pub item_type: String,
    /// Stack size for this item.
    pub amount: u64,
    pub tradable: bool,
    pub marketable: bool,
}

impl InventoryItem {
    /// Best display name: the visible `name`, falling back to the market name.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.market_hash_name.is_empty() {
            self.market_hash_name.clone()
        } else {
            "(unnamed item)".to_string()
        }
    }
}

/// One Workshop item, from the user's subscriptions or a browse page; the
/// listing shows the title and size.
#[derive(Debug, Clone, Default)]
pub struct WorkshopItem {
    /// The published-file id.
    pub id: Option<u64>,
    /// The item title.
    pub title: String,
    /// Whether the item's content is installed on disk. Nothing reports this
    /// yet, so it is always `false`.
    pub installed: bool,
    /// Whether the user is subscribed to the item: `true` for the subscribed
    /// list, `false` for browse results until a subscribe succeeds.
    pub subscribed: bool,
    /// On-disk size in bytes.
    pub file_size: u64,
}

impl WorkshopItem {
    /// Display title, falling back to the published-file id when the title is
    /// missing/empty.
    pub fn display_title(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else if let Some(id) = self.id {
            format!("Item {}", id)
        } else {
            "(untitled)".to_string()
        }
    }
}

/// One comment on a Workshop item: its author (a SteamID), the text and a
/// Unix timestamp.
#[derive(Debug, Clone, Default)]
pub struct WorkshopComment {
    /// The commenter's display name.
    pub author: String,
    /// The comment body.
    pub message: String,
    /// Unix timestamp (seconds) the comment was posted.
    pub timestamp: i64,
}

impl WorkshopComment {
    /// Display author, falling back to a placeholder when none is present.
    pub fn display_author(&self) -> String {
        if self.author.is_empty() {
            "(anonymous)".to_string()
        } else {
            self.author.clone()
        }
    }
}

/// One DLC of a base game. The status fields are `None` when the best-effort
/// state lookup found nothing.
#[derive(Debug, Clone)]
pub struct Dlc {
    pub app_id: u32,
    pub name: Option<String>,
    // `None` when the best-effort `DlcState` lookup found nothing; helpers
    // expose a defaulted view.
    pub owned: Option<bool>,
    pub installed: Option<bool>,
    pub disabled: Option<bool>,
}

impl Dlc {
    /// Display name, falling back to the app id when the store name is missing.
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("App {}", self.app_id))
    }

    pub fn is_owned(&self) -> bool {
        self.owned.unwrap_or(false)
    }

    pub fn is_installed(&self) -> bool {
        self.installed.unwrap_or(false)
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }
}

/// One beta branch of a game. Steam reports branches by name only
/// (password-gated ones are filtered out server-side).
#[derive(Debug, Clone, Default)]
pub struct Branch {
    pub name: String,
    pub description: String,
    /// Whether this is the branch the install is currently tracking. The current
    /// branch list doesn't say, so it starts `false`; `Browser::open_branches`
    /// marks the active branch by name when one is known.
    pub active: bool,
}

/// One depot of a game.
#[derive(Debug, Clone, Default)]
pub struct Depot {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    pub config: String,
    pub is_owned: Option<bool>,
}

/// One launch option of a game: one of the ways Steam can start it (e.g. a
/// normal launch vs. a level editor).
#[derive(Debug, Clone, Default)]
pub struct LaunchOption {
    /// The launch option's id (a string in the app's manifest).
    pub id: String,
    /// Human-readable label for this launch option.
    pub description: String,
    /// The executable Steam would run.
    pub executable: String,
    /// Arguments passed to the executable.
    pub arguments: String,
    /// Working directory for the launch, if specified.
    pub working_dir: String,
    /// Target OS list (e.g. "windows"); empty means any.
    pub oslist: String,
    /// Target OS architecture (e.g. "64"); empty means any.
    pub osarch: String,
    /// Launch option type (e.g. "default", "option1").
    pub launch_type: String,
}

impl LaunchOption {
    /// Display label, falling back to the executable then the id when the
    /// store-provided description is missing.
    pub fn display_name(&self) -> String {
        if !self.description.is_empty() {
            self.description.clone()
        } else if !self.executable.is_empty() {
            self.executable.clone()
        } else {
            format!("Option {}", self.id)
        }
    }

    /// The command line (executable + arguments), trimmed; empty when neither is set.
    pub fn command(&self) -> String {
        format!("{} {}", self.executable, self.arguments)
            .trim()
            .to_string()
    }

    /// The OS tag for this option ("any" when no OS list is specified).
    pub fn os_tag(&self) -> String {
        if self.oslist.is_empty() {
            "any".to_string()
        } else {
            self.oslist.clone()
        }
    }
}

/// One row in the market overlay: a single active sell listing or open buy
/// order; `market_listings` merges both into one list and tags each row's
/// `kind`. `price` is in the currency's
/// minor units (cents); `quantity` is only meaningful for buy orders.
#[derive(Debug, Clone, Default)]
pub struct MarketListing {
    /// The item's market hash name.
    pub name: String,
    /// Price in the currency's minor units (cents).
    pub price: u64,
    /// Quantity still wanted (buy orders only; 1 for listings).
    pub quantity: u64,
    /// A short tag we set ourselves: "listing" or "buy order".
    pub kind: String,
}

impl MarketListing {
    /// The price formatted from minor units to a major-unit decimal (e.g. 199 -> "1.99").
    pub fn price_text(&self) -> String {
        format!("{}.{:02}", self.price / 100, self.price % 100)
    }
}

/// One Community Market search result. The search needs no login.
#[derive(Debug, Clone, Default)]
pub struct MarketSearchResult {
    /// The app the item belongs to (used for the follow-up price lookup).
    pub app_id: u32,
    /// The app's name (e.g. "Team Fortress 2").
    pub app_name: String,
    /// The exact market hash name (what a price lookup needs).
    pub market_hash_name: String,
    /// The display name (falls back to `market_hash_name`).
    pub name: String,
    /// How many sell listings are open for the item.
    pub sell_listings: u64,
    /// The lowest sell price in the currency's minor units (cents).
    pub sell_price: u64,
    /// The lowest sell price, pre-formatted by Steam (e.g. "$2.34").
    pub sell_price_text: String,
}

impl MarketSearchResult {
    /// Best display name: the visible `name`, falling back to the market hash
    /// name, then a placeholder.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.market_hash_name.is_empty() {
            self.market_hash_name.clone()
        } else {
            "(unnamed item)".to_string()
        }
    }

    /// The price text, preferring Steam's pre-formatted string and falling
    /// back to a major-unit decimal of `sell_price` (cents). Empty when neither
    /// is known.
    pub fn price_text(&self) -> String {
        if !self.sell_price_text.is_empty() {
            self.sell_price_text.clone()
        } else if self.sell_price > 0 {
            format!("{}.{:02}", self.sell_price / 100, self.sell_price % 100)
        } else {
            String::new()
        }
    }
}

/// A Community Market price lookup. It needs no login; an empty result is
/// rendered as "no data".
#[derive(Debug, Clone, Default)]
pub struct MarketPrice {
    /// The market hash name the price is for.
    pub market_hash_name: String,
    /// Lowest current sell price (pre-formatted, e.g. "$2.34").
    pub lowest_price: Option<String>,
    /// Median recent sale price (pre-formatted).
    pub median_price: Option<String>,
    /// 24-hour sales volume, as Steam formats it (e.g. "69,730").
    pub volume: Option<String>,
}

impl MarketPrice {
    /// A one-line summary of the known price fields, suitable for a status line.
    /// Returns `None` when the payload carried no usable data.
    pub fn summary(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(p) = self.lowest_price.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("low {}", p));
        }
        if let Some(p) = self.median_price.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("median {}", p));
        }
        if let Some(v) = self.volume.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("vol {}", v));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("   "))
        }
    }
}

/// Search the Community Market (no login needed) and publish the outcome,
/// tagged with `search_gen`, into `cell`. Intended to run on a dedicated
/// thread so the UI stays responsive.
#[cfg(feature = "market")]
pub fn market_search_async(
    query: String,
    search_gen: u64,
    cell: Arc<Mutex<(u64, MarketSearchState)>>,
) {
    let result =
        in_process(libgaki::ops::market::search(Some(&query), None, 20)).map(|(_, results)| {
            results
                .into_iter()
                .map(|r| MarketSearchResult {
                    app_id: r.app_id,
                    app_name: r.app_name,
                    market_hash_name: r.market_hash_name,
                    name: r.name,
                    sell_listings: u64::from(r.sell_listings),
                    sell_price: r.sell_price,
                    sell_price_text: r.sell_price_text,
                })
                .collect()
        });
    if let Ok(mut slot) = cell.lock() {
        *slot = (
            search_gen,
            match result {
                Ok(results) => MarketSearchState::Ready(results),
                Err(err) => MarketSearchState::Failed(err.to_string()),
            },
        );
    }
}

/// Look up an item's Community Market price in USD (no login needed) and
/// publish the outcome into `cell`. Intended to run on a dedicated thread.
#[cfg(feature = "market")]
pub fn market_price_async(app_id: u32, name: String, cell: Arc<Mutex<MarketPriceState>>) {
    let result = in_process(libgaki::ops::market::price(app_id, &name, 1)).map(|p| MarketPrice {
        market_hash_name: p.market_hash_name,
        lowest_price: p.lowest_price,
        median_price: p.median_price,
        volume: p.volume,
    });
    if let Ok(mut slot) = cell.lock() {
        *slot = match result {
            Ok(price) => MarketPriceState::Ready(price),
            Err(err) => MarketPriceState::Failed(err.to_string()),
        };
    }
}

/// The state of an in-flight (or completed) Community Market search, shared
/// between the UI thread and the worker thread via an `Arc<Mutex<..>>` cell.
#[derive(Debug, Clone, Default)]
pub enum MarketSearchState {
    /// No search has been run yet.
    #[default]
    Idle,
    /// A search is running; the worker thread has not published a result.
    Loading,
    /// The search completed with these results (possibly empty).
    Ready(Vec<MarketSearchResult>),
    /// The search failed; the `String` is the error message.
    Failed(String),
}

/// The state of an in-flight (or completed) Community Market price lookup, shared
/// between the UI thread and the worker thread via an `Arc<Mutex<..>>` cell.
#[derive(Debug, Clone, Default)]
pub enum MarketPriceState {
    /// No lookup has been run yet.
    #[default]
    Idle,
    /// A lookup is running; the worker thread has not published a result.
    Loading,
    /// The lookup completed with this price.
    Ready(MarketPrice),
    /// The lookup failed; the `String` is the error message.
    Failed(String),
}

/// One Proton/Wine runtime: an official build Steam can install, or a runtime
/// found on disk (custom builds such as GE come from the package manager).
#[derive(Debug, Clone, Default)]
pub struct ProtonRuntime {
    /// The runtime name, used to select it (e.g. `Proton 9.0`, `GE-Proton10-34`).
    pub name: String,
    /// Whether this is an official build, installed and removed through Steam.
    pub official: bool,
    /// Whether this runtime is present on disk.
    pub installed: bool,
    /// Whether this runtime is the configured global default.
    pub is_default: bool,
}

impl ProtonRuntime {
    /// Whether this runtime can be uninstalled here: only an installed official
    /// build. Custom builds belong to the package manager.
    pub fn uninstallable(&self) -> bool {
        self.installed && self.official
    }
}

/// One game gaki launched that is still running.
#[derive(Debug, Clone, Default)]
pub struct RunningGame {
    pub app_id: u32,
    pub name: String,
    pub pid: u32,
}

/// Where a classic (username/password) login currently stands. Drives what the
/// TUI shows and when it prompts for a Steam Guard code.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginPhase {
    Idle,
    Connecting,
    /// Login is blocked waiting for the user to approve it on their device, or
    /// to type a code (which may arrive next as `GuardCode`).
    AwaitingConfirmation,
    /// A typed Steam Guard code is required; the `String` is its kind
    /// (`"email"` or `"device"`).
    GuardCode(String),
    /// Approval must happen in the Steam Mobile app (no typed code).
    DeviceConfirmation,
    Success,
    Failed(String),
}

/// Format a download ETA into a compact " · ETA 3m 12s" suffix to sit next to
/// the State value. Empty when the time is not yet estimable, so a status line
/// without an estimate is unchanged.
fn eta_suffix(eta_seconds: Option<u64>) -> String {
    let Some(secs) = eta_seconds else {
        return String::new();
    };
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let pretty = if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    };
    format!(" · ETA {pretty}")
}

/// Run a `libgaki` call in-process on the shared worker (see
/// [`crate::app::worker`]). Blocks the calling thread like the subprocess
/// calls it replaces.
fn in_process<T>(
    future: impl std::future::Future<Output = anyhow::Result<T>>,
) -> Result<T, TuiError> {
    worker::global()
        .block_on(future)
        .map_err(|e| TuiError::Problem(format!("{e:#}")))
}

/// Report whether a Steam session is currently authenticated.
pub fn health() -> Result<SessionHealth, TuiError> {
    in_process(async {
        let client = worker::global().session().ensure().await?;
        let status = libgaki::ops::auth::session_status(Some(&client)).await;
        Ok(SessionHealth {
            logged_in: status.authenticated,
            account: status.account,
            steam_id: status.steam_id,
            daemon: false,
        })
    })
}

/// Fetch the logged-in Steam account.
pub fn account() -> Result<Account, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let data = libgaki::ops::library::account(&client).await;
        Ok(Account {
            steam_id: data.steam_id,
            account_name: data.account_name,
            persona_name: data.persona_name.filter(|n| !n.is_empty()),
            country: data.country,
            email: data.email,
            email_validated: data.email_validated,
            authed_machines: data.authed_machines,
            vac_bans: data.vac_bans,
        })
    })
}

/// Fetch the launcher configuration.
pub fn config_show() -> Result<LauncherSettings, TuiError> {
    use libgaki::core::config::ChatPresence;
    let config = in_process(libgaki::ops::config::load())?;
    let text = |s: String| Some(s).filter(|s| !s.is_empty());
    Ok(LauncherSettings {
        steam_library_path: text(config.steam_library_path),
        proton_version: text(config.proton_version),
        chat_presence: Some(
            match config.chat_presence {
                ChatPresence::Online => "online",
                ChatPresence::Offline => "offline",
            }
            .to_string(),
        ),
        enable_cloud_sync: config.enable_cloud_sync,
    })
}

/// Set the friends/chat presence, and announce it right away when logged in.
pub fn set_presence(online: bool) -> Result<(), TuiError> {
    use libgaki::core::config::ChatPresence;
    in_process(async {
        libgaki::ops::config::set_presence(if online {
            ChatPresence::Online
        } else {
            ChatPresence::Offline
        })?;
        // Best-effort: the saved setting applies on the next connect anyway.
        if let Some(client) = worker::global().session().client().await
            && client.is_authenticated()
        {
            let _ = client.announce_configured_presence().await;
        }
        Ok(())
    })
}

/// The network proxy used for all HTTP(S) communication (Steam web endpoints,
/// depot downloads, Proton lookups). The Steam CM connection is not proxied.
#[derive(Debug, Clone, Default)]
pub struct ProxySettings {
    /// The proxy URL (e.g. `http://host:8080`, `socks5://host:1080`), or `None`
    /// for a direct connection.
    pub url: Option<String>,
    /// Comma-separated hosts/domains that bypass the proxy (`NO_PROXY`).
    pub no_proxy: Option<String>,
}

/// Apply a proxy change and return the resulting setting.
fn update_proxy(change: libgaki::ops::config::ProxyChange) -> Result<ProxySettings, TuiError> {
    let proxy = in_process(libgaki::ops::config::update_proxy(change))?;
    Ok(ProxySettings {
        url: proxy.url,
        no_proxy: proxy.no_proxy,
    })
}

/// Show the configured HTTP(S) proxy without changing it.
pub fn config_proxy_show() -> Result<ProxySettings, TuiError> {
    update_proxy(Default::default())
}

/// Set the proxy URL (validated); an empty string reverts to a direct
/// connection.
pub fn config_proxy_set(url: &str) -> Result<(), TuiError> {
    update_proxy(libgaki::ops::config::ProxyChange {
        url: Some(url.to_string()),
        ..Default::default()
    })
    .map(drop)
}

/// Set the proxy bypass list (`NO_PROXY`).
pub fn config_proxy_set_no_proxy(list: &str) -> Result<(), TuiError> {
    update_proxy(libgaki::ops::config::ProxyChange {
        no_proxy: Some(list.to_string()),
        ..Default::default()
    })
    .map(drop)
}

/// Clear the configured proxy, reverting to a direct connection.
pub fn config_proxy_clear() -> Result<(), TuiError> {
    update_proxy(libgaki::ops::config::ProxyChange {
        clear: true,
        ..Default::default()
    })
    .map(drop)
}

/// Revoke the refresh token at Steam, log out, and delete the stored
/// session. A failed revocation is logged; the local session goes anyway.
pub fn logout() -> Result<(), TuiError> {
    in_process(async {
        let session = worker::global().session();
        // A live session revokes without a new logon; otherwise logout
        // restores only to revoke, so a broken session can still be removed.
        let mut client = match session.client().await {
            Some(client) if client.is_authenticated() => client,
            _ => libgaki::steam_client::SteamClient::new()?,
        };
        let result = libgaki::ops::auth::logout(&mut client).await;
        session.reset().await;
        result.map(drop)
    })
}

/// The Steam Wallet balance.
#[cfg(feature = "inventory")]
pub fn wallet() -> Result<Wallet, TuiError> {
    in_process(async {
        let grant = libgaki::ops::auth::sensitive_grant()?;
        let web = web_session().await?;
        let w = libgaki::ops::market::wallet(&web, grant).await?;
        Ok(Wallet {
            balance_cents: w.balance_cents,
            currency: w.currency,
            country: w.country,
            formatted: w.formatted,
        })
    })
}

/// A web session for the web-surface calls (inventory, wallet, market
/// listings): minted off the logged-in session, or built from a stored
/// browser web token.
#[cfg(feature = "inventory")]
async fn web_session() -> anyhow::Result<libgaki::web::web_session::WebSession> {
    let client = worker::global().session().ensure().await?;
    libgaki::ops::session::web_session(&client).await
}

/// Fetch the full library.
pub fn fetch_library() -> Result<Vec<LibraryGame>, TuiError> {
    use libgaki::core::steam_urls;
    in_process(async {
        let mut client = worker::global().session().ensure().await?;
        let options = libgaki::ops::library::ListOptions::default();
        let listing = libgaki::ops::library::list(&mut client, &options).await?;
        Ok(listing
            .games
            .into_iter()
            .map(|g| LibraryGame {
                assets: Some(ArtworkUrls {
                    header: Some(steam_urls::header_url(g.app_id)),
                    capsule: Some(steam_urls::capsule_url(g.app_id)),
                    hero: Some(steam_urls::hero_url(g.app_id)),
                    logo: Some(steam_urls::logo_url(g.app_id)),
                }),
                store_url: Some(steam_urls::store_url(g.app_id)),
                app_id: g.app_id,
                name: g.name,
                is_installed: g.is_installed,
                install_path: g.install_path,
                update_available: g.update_available,
                is_owned: g.is_owned,
                is_family_shared: g.is_family_shared,
                platform: g.platform,
                active_branch: Some(g.active_branch),
            })
            .collect())
    })
}

/// One Steam library folder (a root containing `steamapps`), with the free
/// space on its drive.
#[derive(Debug, Clone)]
pub struct LibraryFolder {
    pub path: String,
    /// Free bytes on the drive, if they could be determined.
    pub free_bytes: Option<u64>,
}

/// List the Steam library folders games can be installed into, one per
/// drive/location, with each drive's free space.
pub fn libraries() -> Result<Vec<LibraryFolder>, TuiError> {
    in_process(async {
        Ok(libgaki::ops::library::folders()
            .await
            .into_iter()
            .map(|f| LibraryFolder {
                path: f.path.to_string_lossy().into_owned(),
                free_bytes: f.free_bytes,
            })
            .collect())
    })
}

/// Estimate the on-disk install size of a game, in bytes. Returns 0 if the
/// estimate is unavailable.
#[cfg(feature = "install")]
pub fn install_estimate(id: i32) -> Result<u64, TuiError> {
    in_process(async {
        let mut client = worker::global().session().authed().await?;
        let estimate = libgaki::ops::install::estimate(&mut client, id as u32, None).await?;
        Ok(estimate.size.disk_size)
    })
}

/// Fetch store metadata for a single app.
pub fn fetch_info(id: i32) -> Result<StoreInfo, TuiError> {
    use libgaki::ops::info::{InfoRequest, app_info};
    in_process(async {
        let request = InfoRequest {
            app_ids: vec![id as u32],
            extended: false,
            no_cache: false,
            country: libgaki::ops::info::resolve_country(None)?,
        };
        let session = worker::global().session();
        let mut infos = app_info(&request, async || session.authed().await).await?;
        let details = infos
            .pop()
            .ok_or_else(|| anyhow::anyhow!("no store data for app {id}"))?
            .details;
        Ok(StoreInfo {
            app_id: details.app_id,
            name: details.name,
            app_type: details.app_type,
            developers: details.developers,
            publishers: details.publishers,
            description: details.short_description,
            reviews: details.review_summary,
        })
    })
}

/// List a game's Steam Cloud files, sorted by name.
#[cfg(feature = "cloud")]
pub fn cloud_list(app_id: i32) -> Result<Vec<CloudFile>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        Ok(libgaki::ops::cloud::files(&client, app_id as u32)
            .await?
            .into_iter()
            .map(|f| CloudFile {
                filename: f.filename,
                size: f.size,
                timestamp: f.timestamp as i64,
            })
            .collect())
    })
}

/// Which way a Steam Cloud sync should move files.
#[cfg(feature = "cloud")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudDirection {
    /// Download then upload.
    Both,
    /// Download only.
    Down,
    /// Upload only.
    Up,
}

/// Sync a game's Steam Cloud saves, optionally in one direction only. A
/// blocking call that can be slow. Diverged saves are left alone (both copies
/// kept); they, and files that failed or had no local path, come back as an
/// error saying so, rather than as a plain success.
#[cfg(feature = "cloud")]
pub fn cloud_sync(app_id: i32, direction: CloudDirection) -> Result<(), TuiError> {
    use libgaki::library::cloud_sync::{ConflictPolicy, SyncDirection};
    let request = libgaki::ops::cloud::SyncRequest {
        app_id: app_id as u32,
        direction: match direction {
            CloudDirection::Both => SyncDirection::Both,
            CloudDirection::Down => SyncDirection::Down,
            CloudDirection::Up => SyncDirection::Up,
        },
        policy: ConflictPolicy::Detect,
        remote_root: None,
    };
    let report = in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::cloud::sync(&client, &request).await
    })?;
    match sync_problem(app_id, &report.outcome) {
        Some(problem) => Err(TuiError::Problem(problem)),
        None => Ok(()),
    }
}

/// What went wrong in a sync, worst first, or `None` if nothing did.
#[cfg(feature = "cloud")]
fn sync_problem(
    app_id: i32,
    outcome: &libgaki::library::cloud_sync::SyncOutcome,
) -> Option<String> {
    if let Some(first) = outcome.failed.first() {
        return Some(format!(
            "{} file(s) failed to transfer (first: {}: {})",
            outcome.failed.len(),
            first.filename,
            first.error
        ));
    }
    if outcome.has_conflicts() {
        return Some(format!(
            "{} save(s) differ between Steam Cloud and this machine; both copies were kept. \
             Resolve with `gaki-cli cloud sync {app_id} --resolve cloud|local`",
            outcome.conflicts.len(),
        ));
    }
    if let Some(first) = outcome.skipped.first() {
        return Some(format!(
            "{} file(s) skipped (first: {}: {})",
            outcome.skipped.len(),
            first.filename,
            first.reason
        ));
    }
    None
}

/// A game's achievements with the user's unlock state and global rarity.
pub fn achievements(app_id: i32) -> Result<Vec<Achievement>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let list = libgaki::ops::info::achievements(&client, app_id as u32).await?;
        Ok(list
            .into_iter()
            .map(|a| Achievement {
                name: a.name,
                description: a.description,
                unlocked: a.unlocked,
                visible: !a.hidden,
                rarity: a.global_percent,
            })
            .collect())
    })
}

/// The logged-in user's friends, with status and current game. The roster
/// fills in over a few seconds after connecting, so this may be partial.
#[cfg(feature = "social")]
pub fn friends() -> Result<Vec<Friend>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let friends =
            libgaki::ops::social::friends(&client, libgaki::ops::social::FRIENDS_WAIT).await?;
        Ok(friends
            .into_iter()
            .map(|f| Friend {
                steam_id: f.steam_id,
                persona_name: f.persona_name,
                persona_state: f.persona_state,
                game_app_id: f.game_app_id,
                game_name: f.game_name,
            })
            .collect())
    })
}

/// Look up a Steam user by SteamID, profile URL, vanity name or friend name.
/// Read-only.
#[cfg(feature = "social")]
pub fn friends_search(query: &str) -> Result<FriendLookup, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let steam_id = libgaki::ops::session::resolve_user_ident(&client, query, None).await?;
        let profile = libgaki::ops::social::profile(&client, steam_id).await?;
        Ok(FriendLookup {
            steam_id: profile.steam_id,
            persona_name: profile.persona_name,
            profile_url: Some(profile.profile_url).filter(|u| !u.is_empty()),
        })
    })
}

/// Send a friend request to the user `query` resolves to.
#[cfg(feature = "social")]
pub fn friends_add(query: &str) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let steam_id = libgaki::ops::session::resolve_user_ident(&client, query, None).await?;
        libgaki::ops::social::add_friend(&client, steam_id).await
    })
    .map(drop)
}

/// Remove a friend or cancel a pending request.
#[cfg(feature = "social")]
pub fn friends_remove(steamid: u64) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::social::remove_friend(&client, steamid).await
    })
}

/// The most recent `count` messages with a friend, oldest first.
#[cfg(feature = "social")]
pub fn chat_history(steamid: u64, count: u32) -> Result<Vec<ChatMessage>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let messages = libgaki::ops::social::history(&client, steamid, count).await?;
        Ok(messages
            .into_iter()
            .map(|m| ChatMessage {
                sender: m.sender,
                from_self: m.from_self,
                message: m.message,
                timestamp: i64::from(m.timestamp),
            })
            .collect())
    })
}

/// Send a direct message to a friend.
#[cfg(feature = "social")]
pub fn chat_send(steamid: u64, message: &str) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::social::send_message(&client, steamid, message.to_string()).await
    })
    .map(drop)
}

/// The account's inventory for a game (context 2, the game-items context).
#[cfg(feature = "inventory")]
pub fn inventory(app_id: i32) -> Result<Vec<InventoryItem>, TuiError> {
    in_process(async {
        let grant = libgaki::ops::auth::sensitive_grant()?;
        let web = web_session().await?;
        let items = libgaki::ops::market::inventory(&web, app_id as u32, 2, grant).await?;
        Ok(items
            .into_iter()
            .map(|i| InventoryItem {
                name: i.name,
                market_hash_name: i.market_hash_name,
                item_type: i.item_type,
                amount: i.amount,
                tradable: i.tradable,
                marketable: i.marketable,
            })
            .collect())
    })
}

/// A base game's DLC with ownership and install state.
pub fn dlc(app_id: i32) -> Result<Vec<Dlc>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let list = libgaki::ops::info::dlc_list(&client, app_id as u32).await?;
        Ok(list
            .into_iter()
            .map(|d| Dlc {
                app_id: d.app_id,
                name: d.name,
                owned: d.state.as_ref().map(|s| s.owned),
                installed: d.state.as_ref().map(|s| s.installed),
                disabled: d.state.as_ref().map(|s| s.disabled),
            })
            .collect())
    })
}

/// A game's depots.
pub fn depots(app_id: i32) -> Result<Vec<Depot>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let list = libgaki::ops::info::depots(&client, app_id as u32).await?;
        Ok(list
            .into_iter()
            .map(|d| Depot {
                id: d.id,
                name: d.name,
                size: d.size,
                file_count: d.file_count,
                config: d.config,
                is_owned: d.is_owned,
            })
            .collect())
    })
}

/// A game's launch options, as listed in its app info.
pub fn launch_options(app_id: i32) -> Result<Vec<LaunchOption>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let list = libgaki::ops::info::launch_options(&client, app_id as u32).await?;
        Ok(list
            .into_iter()
            .map(|o| LaunchOption {
                id: o.id,
                description: o.description,
                executable: o.executable,
                arguments: o.arguments,
                working_dir: o.working_dir,
                oslist: o.oslist,
                osarch: o.osarch,
                launch_type: o.launch_type,
            })
            .collect())
    })
}

/// The Workshop items the user is subscribed to for a game.
#[cfg(feature = "workshop")]
pub fn workshop_list(app_id: i32) -> Result<Vec<WorkshopItem>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let items = libgaki::ops::workshop::subscribed(&client, app_id as u32).await?;
        Ok(items.into_iter().map(|i| workshop_row(i, true)).collect())
    })
}

/// A Workshop item as the overlay shows it.
#[cfg(feature = "workshop")]
fn workshop_row(item: libgaki::core::models::WorkshopItem, subscribed: bool) -> WorkshopItem {
    WorkshopItem {
        id: Some(item.id),
        title: item.title,
        installed: false,
        subscribed,
        file_size: item.file_size,
    }
}

/// Browse a game's Workshop, optionally filtered by `search`. Rows start as
/// not subscribed; the overlay flips the flag on a successful subscribe.
#[cfg(feature = "workshop")]
pub fn workshop_browse(app_id: i32, search: &str) -> Result<Vec<WorkshopItem>, TuiError> {
    let search = Some(search.trim().to_string()).filter(|s| !s.is_empty());
    in_process(async {
        let client = worker::global().session().authed().await?;
        let query = libgaki::ops::workshop::Query {
            app_id: app_id as u32,
            search,
            sort: libgaki::ops::workshop::Sort::Trend,
            cursor: "*".to_string(),
            count: 20,
            tags: Vec::new(),
        };
        let page = libgaki::ops::workshop::browse(&client, &query).await?;
        Ok(page
            .items
            .into_iter()
            .map(|i| workshop_row(i, false))
            .collect())
    })
}

/// Subscribe to a Workshop item (a collection subscribes to its items too).
#[cfg(feature = "workshop")]
pub fn workshop_subscribe(item_id: u64) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        for item in libgaki::ops::workshop::resolve(&client, vec![item_id], true).await? {
            libgaki::ops::workshop::subscribe(&client, &item).await?;
        }
        Ok(())
    })
}

/// Unsubscribe from a Workshop item (a collection unsubscribes its items too).
#[cfg(feature = "workshop")]
pub fn workshop_unsubscribe(item_id: u64) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        for item in libgaki::ops::workshop::resolve(&client, vec![item_id], true).await? {
            libgaki::ops::workshop::unsubscribe(&client, &item).await?;
        }
        Ok(())
    })
}

/// Vote a Workshop item up or down.
#[cfg(feature = "workshop")]
pub fn workshop_rate(item_id: u64, up: bool) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::workshop::rate(&client, item_id, up).await
    })
}

/// The first `count` comments on a Workshop item. Authors are SteamIDs.
#[cfg(feature = "workshop")]
pub fn workshop_comments(item_id: u64, count: u32) -> Result<Vec<WorkshopComment>, TuiError> {
    let count = i32::try_from(count).unwrap_or(i32::MAX);
    in_process(async {
        let client = worker::global().session().authed().await?;
        let comments = libgaki::ops::workshop::comments(&client, item_id, 0, count).await?;
        Ok(comments
            .into_iter()
            .map(|c| WorkshopComment {
                author: c.author.to_string(),
                message: c.text,
                timestamp: c.timestamp,
            })
            .collect())
    })
}

/// The account's active market listings and open buy orders, as one list.
#[cfg(feature = "market")]
pub fn market_listings() -> Result<Vec<MarketListing>, TuiError> {
    let state = in_process(async {
        let grant = libgaki::ops::auth::sensitive_grant()?;
        let web = web_session().await?;
        libgaki::ops::market::listings(&web, grant).await
    })?;
    let mut rows = Vec::with_capacity(state.listings.len() + state.buy_orders.len());
    for l in state.listings {
        rows.push(MarketListing {
            name: l.market_hash_name,
            price: l.price,
            quantity: 1,
            kind: "listing".to_string(),
        });
    }
    for b in state.buy_orders {
        rows.push(MarketListing {
            name: b.market_hash_name,
            price: b.price,
            quantity: b.quantity,
            kind: "buy order".to_string(),
        });
    }
    Ok(rows)
}

/// Enable or disable a single DLC. Steam reads DLC state only at startup, so
/// a running Steam picks the change up on its next start.
pub fn set_dlc(app_id: i32, enable: bool) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().ensure().await?;
        libgaki::ops::info::set_dlc(&client, app_id as u32, enable).await
    })
    .map(drop)
}

/// A game's beta branches, by name.
#[cfg(feature = "versioning")]
pub fn branches(app_id: i32) -> Result<Vec<Branch>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let names = libgaki::ops::info::branches(&client, app_id as u32).await?;
        Ok(names
            .into_iter()
            .map(|name| Branch {
                name,
                ..Default::default()
            })
            .collect())
    })
}

/// Switch a game to a beta branch. The switch is staged: an [`update`] is
/// needed afterwards for it to take effect.
#[cfg(feature = "versioning")]
pub fn set_branch(app_id: i32, branch: &str) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::info::set_branch(&client, app_id as u32, branch).await
    })
}

/// List the Proton/Wine runtimes: every official build, marked installed and
/// default as they are, then any other runtime found on disk.
pub fn proton_list() -> Result<Vec<ProtonRuntime>, TuiError> {
    let listing = in_process(libgaki::ops::proton::listing())?;
    let is_default = |name: &str| name.eq_ignore_ascii_case(&listing.default);
    let mut protons: Vec<ProtonRuntime> = listing
        .available
        .iter()
        .map(|p| ProtonRuntime {
            name: p.name.clone(),
            official: true,
            installed: listing.is_installed(&p.name),
            is_default: is_default(&p.name),
        })
        .collect();
    // Everything else on disk (custom builds, older releases) is listed too.
    for inst in &listing.installed {
        if !protons
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&inst.name))
        {
            protons.push(ProtonRuntime {
                name: inst.name.clone(),
                official: false,
                installed: true,
                is_default: is_default(&inst.name),
            });
        }
    }
    Ok(protons)
}

/// Make `version` the global default Proton/Wine runtime.
pub fn proton_default(version: &str) -> Result<(), TuiError> {
    in_process(libgaki::ops::proton::set_default(version)).map(drop)
}

/// Install an official Proton build through Steam and make it the default,
/// streaming progress into the shared status cell. Blocks until done; intended
/// to be run on a dedicated thread (mirrors [`install`]).
#[cfg(feature = "install")]
pub fn proton_install(name: String, status: Arc<Mutex<Option<GameStatus>>>) {
    set_status(&status, "processing...");
    let outcome = worker::global().block_on(async {
        let package = libgaki::ops::proton::find(&name)?;
        let client = worker::global().session().authed().await?;
        let rx = libgaki::ops::proton::start_install(&client, &package).await?;
        if !drive_progress(rx, &status, ProgressLabel::Install).await? {
            anyhow::bail!("the install of {} stopped before it finished", package.name);
        }
        libgaki::ops::proton::set_default(&package.name).await?;
        Ok(())
    });
    finish(&status, outcome, "Installed!");
}

/// Uninstall an official Proton build through Steam.
#[cfg(feature = "install")]
pub fn proton_uninstall(name: &str) -> Result<(), TuiError> {
    in_process(async {
        let package = libgaki::ops::proton::find(name)?;
        let client = worker::global().session().authed().await?;
        libgaki::ops::proton::uninstall(&client, &package).await
    })
    .map(drop)
}

/// The games gaki launched that are still running (stale records are dropped).
pub fn running() -> Result<Vec<RunningGame>, TuiError> {
    Ok(libgaki::ops::launch::running()
        .into_iter()
        .map(|g| RunningGame {
            app_id: g.app_id,
            name: g.name,
            pid: g.pid,
        })
        .collect())
}

/// Stop a running game launched by gaki.
pub fn stop(app_id: i32) -> Result<(), TuiError> {
    libgaki::ops::launch::stop(app_id as u32, false)
        .map(|_| ())
        .map_err(|e| TuiError::Problem(format!("{e:#}")))
}

/// Log in by QR code, publishing each challenge URL into `qr_cell` so the UI
/// can render it. Blocks until the login resolves; returns `Ok(())` once
/// authenticated, `Err` on failure, timeout or cancellation. Intended to be
/// run on a dedicated thread. On success the worker session adopts the new
/// client.
///
/// `cancelled` is checked a few times a second: once it returns true the
/// attempt stops (its Steam connection is dropped) and never writes to
/// `qr_cell` again, so an abandoned attempt can't replace a newer one's code.
pub fn login_qr(
    qr_cell: &Arc<Mutex<Option<String>>>,
    cancelled: impl Fn() -> bool,
) -> Result<(), TuiError> {
    use libgaki::core::models::QrEvent;
    in_process(async {
        let mut client = libgaki::steam_client::SteamClient::new()?;
        let login = libgaki::ops::auth::login_qr(
            &mut client,
            libgaki::ops::auth::QR_DEFAULT_TIMEOUT,
            |event| {
                if let QrEvent::Challenge(url) = event
                    && !cancelled()
                    && let Ok(mut slot) = qr_cell.lock()
                {
                    *slot = Some(url.to_string());
                }
            },
        );
        let watch = async {
            while !cancelled() {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        };
        tokio::select! {
            result = login => result?,
            () = watch => anyhow::bail!("QR login cancelled"),
        };
        worker::global().session().set(client).await;
        Ok(())
    })
}

fn set_phase(phase: &Arc<Mutex<LoginPhase>>, value: LoginPhase) {
    if let Ok(mut slot) = phase.lock() {
        *slot = value;
    }
}

/// Classic username/password login.
///
/// Publishes each step into `phase`. When Steam asks for a Steam Guard code
/// the phase becomes [`LoginPhase::GuardCode`] and this blocks on `guard_rx`
/// until the UI supplies it. Intended to run on a dedicated thread; returns
/// `Ok(())` once authenticated, after which the worker session adopts the new
/// client.
pub fn login_classic(
    username: &str,
    password: &str,
    phase: &Arc<Mutex<LoginPhase>>,
    guard_rx: Receiver<String>,
) -> Result<(), TuiError> {
    use libgaki::ops::auth::{LoginStep, PasswordLogin};
    set_phase(phase, LoginPhase::Connecting);
    let result = in_process(async {
        let mut client = libgaki::steam_client::SteamClient::new()?;
        let mut request = PasswordLogin {
            username: username.to_string(),
            password: password.to_string().into(),
            guard_code: None,
            read_code_from_stdin: false,
            timeout: Some(libgaki::ops::auth::LOGIN_TIMEOUT),
        };
        // The attempt blocks while Steam waits for a Mobile-app approval, so
        // say that up front.
        set_phase(phase, LoginPhase::AwaitingConfirmation);
        loop {
            match libgaki::ops::auth::login(&mut client, &request).await? {
                LoginStep::LoggedIn => break,
                LoginStep::CodeRequired(kind) if request.guard_code.is_none() => {
                    set_phase(phase, LoginPhase::GuardCode(kind.as_str().to_string()));
                    // `block_on` polls this future on the calling (login)
                    // thread, so waiting here blocks neither the UI nor the
                    // runtime's workers.
                    let code = guard_rx
                        .recv()
                        .map_err(|_| anyhow::anyhow!("login cancelled"))?;
                    request.guard_code = Some(code.trim().to_string());
                    set_phase(phase, LoginPhase::Connecting);
                }
                LoginStep::CodeRequired(_) => {
                    anyhow::bail!("login failed after providing Steam Guard code")
                }
                LoginStep::ApprovalRequired => {
                    set_phase(phase, LoginPhase::DeviceConfirmation);
                    anyhow::bail!("approve this login in the Steam Mobile app, then log in again")
                }
            }
        }
        worker::global().session().set(client).await;
        Ok(())
    });
    match &result {
        Ok(()) => set_phase(phase, LoginPhase::Success),
        Err(e) => set_phase(phase, LoginPhase::Failed(e.to_string())),
    }
    result
}

/// Update a shared status cell, preserving any previously-known install dir/size.
fn set_status(status: &Arc<Mutex<Option<GameStatus>>>, msg: &str) {
    if let Ok(mut guard) = status.lock() {
        let next = GameStatus::msg(&guard, msg);
        *guard = Some(next);
    }
}

/// What the user has asked of an in-flight install. The install worker reads
/// this once the download ends (`install_stop` aborts it) and finalises the
/// status accordingly instead of treating the abort as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallAction {
    /// Install is running normally; finalise on its actual result.
    Running,
    /// User paused it — freeze the status at "paused …" so it can be resumed.
    Paused,
    /// User cancelled it — clear the status (the game reverts to not-installed).
    Stopped,
}

/// A shared, cheaply-cloned handle to an install's [`InstallAction`]. The UI
/// flips it (pause/stop) before issuing `install_stop`; the worker reads it
/// when the process ends.
#[derive(Debug, Clone)]
pub struct InstallControl {
    action: Arc<Mutex<InstallAction>>,
}

impl InstallControl {
    pub fn new() -> Self {
        Self {
            action: Arc::new(Mutex::new(InstallAction::Running)),
        }
    }

    pub fn set(&self, action: InstallAction) {
        if let Ok(mut guard) = self.action.lock() {
            *guard = action;
        }
    }

    pub fn get(&self) -> InstallAction {
        self.action
            .lock()
            .map(|g| *g)
            .unwrap_or(InstallAction::Running)
    }
}

impl Default for InstallControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Stop a running install. The download loop aborts and the partial download
/// stays on disk, so a later `install` resumes it. A no-op (Ok) if no such
/// install is in flight.
#[cfg(feature = "install")]
pub fn install_stop(app_id: i32) -> Result<(), TuiError> {
    signal_install(app_id, false)
}

/// Pause a running install: it stops, and its backlog entry stays `paused`
/// so it can be resumed. A no-op (Ok) if no such install is in flight.
#[cfg(feature = "install")]
pub fn install_pause(app_id: i32) -> Result<(), TuiError> {
    signal_install(app_id, true)
}

#[cfg(feature = "install")]
fn signal_install(app_id: i32, pause: bool) -> Result<(), TuiError> {
    libgaki::ops::install::signal(app_id as u32, pause)
        .map(|_| ())
        .map_err(|e| TuiError::Problem(format!("{e:#}")))
}

/// Freeze the status cell at "paused" (carrying the last-seen percent, if any),
/// preserving the install dir/size.
#[cfg(feature = "install")]
fn set_status_paused(status: &Arc<Mutex<Option<GameStatus>>>) {
    if let Ok(mut guard) = status.lock() {
        let pct = guard
            .as_ref()
            .and_then(|s| s.state.split_whitespace().find(|t| t.ends_with('%')))
            .map(|p| format!("paused {p}"))
            .unwrap_or_else(|| "paused".to_string());
        let next = GameStatus::msg(&guard, &pct);
        *guard = Some(next);
    }
}

/// Install a game, streaming progress into the shared status cell. Blocks
/// until the install finishes; intended to be run on a dedicated thread.
/// `control` lets the UI pause (resumable) or stop (cancel) the install — when
/// set, the worker finalises as paused/cleared instead of reporting the abort
/// as a failure. `library` (a library root from [`libraries`]) targets a
/// specific drive/location; `None` uses the default.
#[cfg(feature = "install")]
pub fn install(
    id: i32,
    status: Arc<Mutex<Option<GameStatus>>>,
    control: InstallControl,
    library: Option<String>,
) {
    use libgaki::ops::install::{InstallRequest, prepare};
    set_status(&status, "processing...");
    let request = InstallRequest {
        app_id: id as u32,
        platform: None,
        restart_steam: false,
        library,
        on_disk: 0,
        auto_attempts: 0,
    };
    let outcome = worker::global().block_on(async {
        let mut client = worker::global().session().authed().await?;
        let prepared = prepare(&mut client, request).await?;
        prepared
            .run(&client, |rx| {
                drive_progress(rx, &status, ProgressLabel::Install)
            })
            .await
    });

    // If the user paused/stopped, the download aborted on their request —
    // finalise on the user's intent, not the aborted result.
    match control.get() {
        InstallAction::Paused => {
            set_status_paused(&status);
            return;
        }
        InstallAction::Stopped => {
            if let Ok(mut guard) = status.lock() {
                *guard = None;
            }
            return;
        }
        InstallAction::Running => {}
    }
    finish(&status, outcome, "Installed!");
}

/// Which operation [`drive_progress`] is reporting, for its status label.
#[derive(Clone, Copy)]
enum ProgressLabel {
    /// Downloading / verifying / moving, as the backend reports it.
    #[cfg(feature = "install")]
    Install,
    #[cfg(feature = "install")]
    Verify,
    Update,
    #[cfg(feature = "versioning")]
    Downgrade,
}

/// Feed `libgaki` download progress into the status cell until the
/// operation completes (`Ok(true)`), fails, or its channel closes (`Ok(false)`).
async fn drive_progress(
    mut rx: tokio::sync::mpsc::Receiver<libgaki::core::models::DownloadProgress>,
    status: &Arc<Mutex<Option<GameStatus>>>,
    label: ProgressLabel,
) -> anyhow::Result<bool> {
    use libgaki::core::models::DownloadProgressState as State;
    use libgaki::ops::progress::{RateTracker, percent_of};
    let mut rate = RateTracker::new();
    while let Some(p) = rx.recv().await {
        match p.state {
            State::Queued => {
                rate.reset();
                set_status(status, "queued 0.0%");
            }
            State::Downloading | State::Verifying | State::Moving => {
                // Disk reuse isn't network speed.
                let (_, eta) = rate.sample(
                    p.bytes_downloaded.saturating_sub(p.reused_bytes),
                    p.total_bytes.saturating_sub(p.reused_bytes),
                );
                let percent = percent_of(p.bytes_downloaded, p.total_bytes);
                let text = match (label, p.state) {
                    #[cfg(feature = "install")]
                    (ProgressLabel::Verify, _) => format!("verifying {percent:.1}%"),
                    (ProgressLabel::Update, _) => {
                        format!("updating {percent:.1}%{}", eta_suffix(eta))
                    }
                    #[cfg(feature = "versioning")]
                    (ProgressLabel::Downgrade, State::Verifying) => {
                        format!("verifying {percent:.1}%")
                    }
                    #[cfg(feature = "versioning")]
                    (ProgressLabel::Downgrade, _) => {
                        format!("downgrading {percent:.1}%{}", eta_suffix(eta))
                    }
                    #[cfg(feature = "install")]
                    (ProgressLabel::Install, State::Verifying) => {
                        format!("verifying {percent:.1}%{}", eta_suffix(eta))
                    }
                    #[cfg(feature = "install")]
                    (ProgressLabel::Install, State::Moving) => {
                        format!("moving {percent:.1}%{}", eta_suffix(eta))
                    }
                    #[cfg(feature = "install")]
                    (ProgressLabel::Install, _) => {
                        format!("downloading {percent:.1}%{}", eta_suffix(eta))
                    }
                };
                set_status(status, &text);
            }
            State::Completed => return Ok(true),
            State::Failed => {
                let message = format!("operation failed: {}", p.current_file);
                return Err(match p.error_kind {
                    Some(kind) => libgaki::core::error::TypedError::new(kind, message).into(),
                    None => anyhow::anyhow!(message),
                });
            }
        }
    }
    Ok(false)
}

/// Settle a status cell on an operation's outcome.
fn finish(status: &Arc<Mutex<Option<GameStatus>>>, outcome: anyhow::Result<()>, done: &str) {
    match outcome {
        Ok(()) => set_status(status, done),
        Err(e) => set_status(status, &format!("Failed: {e:#}")),
    }
}

/// Uninstall a game. Its Wine prefix / compat data is left in place.
#[cfg(feature = "install")]
pub fn uninstall(app_id: i32) -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::install::uninstall(&client, app_id as u32, false).await
    })
}

/// Refuse to edit Steam's files while it runs: it rewrites appmanifests and
/// `libraryfolders.vdf` on exit.
#[cfg(feature = "install")]
fn require_steam_stopped() -> anyhow::Result<()> {
    libgaki::ops::steam::stop_for_edit(false, || {})
        .map(|_| ())
        .map_err(|e| {
            if libgaki::core::error::classify(&e).kind
                == libgaki::core::error::ErrorKind::InvalidInput
            {
                anyhow::anyhow!("Steam is running. Close it first, then try again.")
            } else {
                e
            }
        })
}

/// Relocate an installed game to another Steam library folder. Blocks until
/// the files are moved (which can take a while). Steam must not be running.
/// (`move` is a Rust keyword, hence `move_game`.)
#[cfg(feature = "install")]
pub fn move_game(app_id: i32, library: &str) -> Result<(), TuiError> {
    let library = std::path::PathBuf::from(library);
    in_process(async move {
        require_steam_stopped()?;
        let client = worker::global().session().authed().await?;
        libgaki::ops::install::move_to(&client, app_id as u32, library, |mut rx| async move {
            use libgaki::core::models::DownloadProgressState as State;
            while let Some(p) = rx.recv().await {
                match p.state {
                    State::Completed => return Ok(true),
                    State::Failed => anyhow::bail!("operation failed: {}", p.current_file),
                    _ => {}
                }
            }
            Ok(false)
        })
        .await
    })
}

/// Point Steam at an install that was moved to another library folder by
/// hand. Steam must not be running.
#[cfg(feature = "install")]
pub fn relink(app_id: i32, library: &str) -> Result<(), TuiError> {
    let library = std::path::PathBuf::from(library);
    in_process(async move {
        require_steam_stopped()?;
        let client = worker::global().session().authed().await?;
        libgaki::ops::install::relink(&client, app_id as u32, library).await?;
        Ok(())
    })
}

/// Register an existing on-disk install of a game into a Steam library
/// folder. Steam must not be running. (Named `import_game` for clarity.)
#[cfg(feature = "install")]
pub fn import_game(app_id: i32, library: &str) -> Result<(), TuiError> {
    let library = std::path::PathBuf::from(library);
    in_process(async move {
        require_steam_stopped()?;
        let client = worker::global().session().authed().await?;
        libgaki::ops::install::import(&client, app_id as u32, library, None).await?;
        Ok(())
    })
}

/// Verify the integrity of a game's files, repairing what is wrong and
/// streaming progress into the shared status cell. Blocks until verification
/// finishes; intended to be run on a dedicated thread.
#[cfg(feature = "install")]
pub fn verify(id: i32, status: Arc<Mutex<Option<GameStatus>>>) {
    set_status(&status, "processing...");
    let outcome = worker::global().block_on(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::install::verify(&client, id as u32, |rx| {
            drive_progress(rx, &status, ProgressLabel::Verify)
        })
        .await
    });
    finish(&status, outcome, "verified");
}

/// Update a game to the latest version, streaming progress into the shared
/// status cell. Blocks until the update finishes; intended to be run on a
/// dedicated thread. A pinned game is refused.
#[cfg(feature = "install")]
pub fn update(id: i32, status: Arc<Mutex<Option<GameStatus>>>) {
    set_status(&status, "processing...");
    let outcome = worker::global().block_on(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::install::update(&client, id as u32, false, |rx| {
            drive_progress(rx, &status, ProgressLabel::Update)
        })
        .await
    });
    finish(&status, outcome, "updated");
}

/// Launch a game and wait for it to exit. Intended to be run on a dedicated
/// thread.
pub fn play(id: i32, status: Arc<Mutex<Option<GameStatus>>>) {
    play_with(id, PlayOpts::default(), status);
}

// ===========================================================================
// Version pinning + downgrade, collections, per-game config
// overrides, and launch targets.
// ===========================================================================

/// One depot's current manifest id on a branch, for version discovery ahead
/// of a `downgrade`.
#[derive(Debug, Clone, Default)]
pub struct DepotManifestInfo {
    pub depot_id: u32,
    pub depot_name: Option<String>,
    pub branch: String,
    pub manifest_id: u64,
    pub size: u64,
}

/// A game's install and version-pin state. `pinned_manifests` maps each depot
/// id (as text) to its pinned manifest.
#[derive(Debug, Clone, Default)]
pub struct PinState {
    pub app_id: u32,
    pub available: bool,
    pub install_path: Option<String>,
    pub pinned: bool,
    pub pinned_manifests: std::collections::BTreeMap<String, u64>,
}

/// The result of pinning a game. `manifests` maps each depot id (as text) to
/// the manifest it was pinned to.
#[derive(Debug, Clone, Default)]
pub struct PinResult {
    pub app_id: u32,
    pub pinned: bool,
    pub manifests: std::collections::BTreeMap<String, u64>,
}

/// One library collection. `kind` is `"built-in"`, `"static"`, or
/// `"dynamic"`; a dynamic one has no `app_ids` (Steam computes its members).
#[derive(Debug, Clone, Default)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub dynamic: bool,
    /// Member count (present on `list`).
    pub count: Option<u64>,
    /// Member app ids (present on `show`; empty for a dynamic collection).
    pub app_ids: Vec<u32>,
}

/// A game's `games.toml` settings, and what it launches with.
#[derive(Debug, Clone, Default)]
pub struct GameSettings {
    pub app_id: u32,
    /// The entry's `profile`; `None` uses `default_profile`.
    pub profile: Option<String>,
    /// The entry's `platform`, `"windows"` or `"linux"`; `None` is automatic.
    pub platform: Option<String>,
    /// The entry's `steam`, `"auto"`, `"host"` or `"none"`; `None` uses the
    /// profile's.
    pub steam: Option<String>,
    /// The profile the game launches with.
    pub effective_profile: Option<String>,
    /// The Steam integration the game launches with.
    pub effective_steam: String,
    /// Every wrapper, the profile's first.
    pub wrappers: Vec<String>,
    /// The game's own wrappers.
    pub game_wrappers: Vec<String>,
    /// Every profile's name, for picking one.
    pub profiles: Vec<String>,
}

/// Extra launch options for [`play_with`].
/// All default to unset/false (see [`PlayOpts::default`]).
#[derive(Debug, Clone, Default)]
pub struct PlayOpts {
    /// Force a specific Proton/Wine runner.
    pub proton: Option<String>,
    /// Launch with real Steam integration (the host Steam client).
    pub steam: bool,
    /// Run this installed build, `"linux"` or `"windows"`, for this launch only.
    pub platform: Option<String>,
    /// The launch target's Steam mode, for display. The launch itself follows
    /// [`PlayOpts::steam`].
    pub steam_mode: Option<String>,
    /// Skip the pre-launch update check.
    pub no_update: bool,
}

/// One way an installed game can be started: a native Linux build, or the
/// Windows build under one Steam mode. `flags` are the `play` flags selecting
/// it; `missing` says what is needed when it is not `available` (e.g.
/// `"build"`).
#[derive(Debug, Clone, Default)]
pub struct LaunchTarget {
    pub id: String,
    pub platform: String,
    pub steam_mode: Option<String>,
    pub available: bool,
    pub default: bool,
    pub missing: Option<String>,
    pub flags: Vec<String>,
}

impl LaunchTarget {
    /// A human label, e.g. `"Linux (native)"` or `"Windows · host Steam"`.
    pub fn label(&self) -> String {
        match (self.platform.as_str(), self.steam_mode.as_deref()) {
            ("linux", _) => "Linux (native)".to_string(),
            ("windows", Some("host")) => "Windows · host Steam".to_string(),
            ("windows", Some("none")) => "Windows · standalone".to_string(),
            (p, Some(m)) => format!("{p} · {m}"),
            (p, None) => p.to_string(),
        }
    }

    /// The [`PlayOpts`] that launch this target.
    pub fn play_opts(&self) -> PlayOpts {
        PlayOpts {
            platform: Some(self.platform.clone()),
            steam_mode: self.steam_mode.clone(),
            // The native target's flags carry `--steam` when computed for it.
            steam: self.flags.iter().any(|f| f == "--steam"),
            ..PlayOpts::default()
        }
    }
}

/// List the ways an installed game can be launched.
pub fn launch_targets(app_id: i32) -> Result<Vec<LaunchTarget>, TuiError> {
    in_process(async {
        let mut client = worker::global().session().authed().await?;
        let targets = libgaki::ops::launch::targets(&mut client, app_id as u32, false).await?;
        Ok(targets
            .targets
            .into_iter()
            .map(|t| LaunchTarget {
                id: t.id.to_string(),
                platform: t.platform.to_string(),
                steam_mode: t.steam_mode.map(str::to_string),
                available: t.available,
                default: t.default,
                missing: t.missing.map(str::to_string),
                flags: t.flags,
            })
            .collect())
    })
}

/// Each depot's current manifest id per branch; `depot` restricts it to one
/// depot. Steam only exposes *current* ids — older ones live on SteamDB.
#[cfg(feature = "versioning")]
pub fn manifests(app_id: i32, depot: Option<u32>) -> Result<Vec<DepotManifestInfo>, TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        let list = libgaki::ops::install::manifests(&client, app_id as u32, depot).await?;
        Ok(list
            .into_iter()
            .map(|m| DepotManifestInfo {
                depot_id: m.depot_id,
                depot_name: m.depot_name,
                branch: m.branch,
                manifest_id: m.manifest_id,
                size: m.size,
            })
            .collect())
    })
}

/// A game's install and version-pin state, from local files only (no Steam
/// logon, so per-game polling is cheap).
#[cfg(feature = "versioning")]
pub fn available(app_id: i32) -> Result<PinState, TuiError> {
    let a = in_process(libgaki::ops::install::availability(app_id as u32))?;
    Ok(PinState {
        app_id: app_id as u32,
        available: a.available,
        install_path: a.install_path,
        pinned: a.pinned,
        pinned_manifests: manifest_map(&a.pinned_manifests),
    })
}

/// Depot → manifest, keyed by the depot id as text, in depot order.
#[cfg(feature = "versioning")]
fn manifest_map(map: &std::collections::HashMap<u32, u64>) -> BTreeMap<String, u64> {
    map.iter().map(|(d, m)| (d.to_string(), *m)).collect()
}

/// Pin a game to its currently-installed manifests, holding gaki's updates.
#[cfg(feature = "versioning")]
pub fn pin(app_id: i32) -> Result<PinResult, TuiError> {
    let manifests = in_process(libgaki::ops::install::pin(app_id as u32))?;
    Ok(PinResult {
        app_id: app_id as u32,
        pinned: true,
        manifests: manifest_map(&manifests),
    })
}

/// Remove a game's version pin. Returns whether it had been pinned.
#[cfg(feature = "versioning")]
pub fn unpin(app_id: i32) -> Result<bool, TuiError> {
    in_process(libgaki::ops::install::unpin(app_id as u32))
}

/// Install specific (usually older) depot manifests and pin them — a
/// downgrade — streaming progress into the shared status cell like
/// [`update`]. `overrides` pairs each depot id with the manifest id to
/// install; `branch` records the build's branch, `verify` runs an integrity
/// pass afterward, and `no_pin` skips pinning. Blocks until it finishes;
/// intended to be run on a dedicated thread.
#[cfg(feature = "versioning")]
pub fn downgrade(
    app_id: i32,
    overrides: &[(u32, u64)],
    branch: Option<&str>,
    verify: bool,
    no_pin: bool,
    status: Arc<Mutex<Option<GameStatus>>>,
) {
    set_status(&status, "processing...");
    let request = libgaki::ops::install::Downgrade {
        app_id: app_id as u32,
        manifests: overrides.iter().copied().collect(),
        library: None,
        branch: branch.map(str::to_string),
        verify,
        pin: !no_pin,
    };
    let outcome = worker::global().block_on(async {
        let mut client = worker::global().session().authed().await?;
        libgaki::ops::install::downgrade(&mut client, &request, |rx| {
            drive_progress(rx, &status, ProgressLabel::Downgrade)
        })
        .await
    });
    finish(&status, outcome, "downgraded");
}

/// Map a local-store error to the TUI's error type.
#[cfg(feature = "collections")]
fn local<T>(result: anyhow::Result<T>) -> Result<T, TuiError> {
    result.map_err(|e| TuiError::Problem(format!("{e:#}")))
}

/// A collection as the overlay shows it.
#[cfg(feature = "collections")]
fn collection_row(c: libgaki::ops::collections::Summary) -> Collection {
    use libgaki::ops::collections::Kind;
    Collection {
        count: Some(c.count() as u64),
        kind: c.kind.as_str().to_string(),
        dynamic: c.kind == Kind::Dynamic,
        app_ids: c.app_ids.unwrap_or_default(),
        id: c.id,
        name: c.name,
    }
}

/// App ids as the store keeps them; negative ids can't be Steam apps.
#[cfg(feature = "collections")]
fn app_ids(ids: &[i32]) -> Result<Vec<u32>, TuiError> {
    ids.iter()
        .map(|&id| u32::try_from(id).map_err(|_| TuiError::Problem(format!("bad app id {id}"))))
        .collect()
}

/// The local library collections (not deleted).
#[cfg(feature = "collections")]
pub fn collections_list() -> Result<Vec<Collection>, TuiError> {
    let (_, list) = local(libgaki::ops::collections::list())?;
    Ok(list.into_iter().map(collection_row).collect())
}

/// One collection by name or id, with its members (none for a dynamic one).
#[cfg(feature = "collections")]
pub fn collection_show(name: &str) -> Result<Collection, TuiError> {
    local(libgaki::ops::collections::show(name)).map(collection_row)
}

/// Create a new static collection.
#[cfg(feature = "collections")]
pub fn collection_create(name: &str) -> Result<(), TuiError> {
    local(libgaki::ops::collections::create(name)).map(drop)
}

/// Mark a collection for deletion; the next push removes it from Steam.
#[cfg(feature = "collections")]
pub fn collection_delete(name: &str) -> Result<(), TuiError> {
    local(libgaki::ops::collections::delete(name))
}

/// Rename a collection.
#[cfg(feature = "collections")]
pub fn collection_rename(name: &str, new_name: &str) -> Result<(), TuiError> {
    local(libgaki::ops::collections::rename(name, new_name))
}

/// Add games to a collection.
#[cfg(feature = "collections")]
pub fn collection_add(name: &str, ids: &[i32]) -> Result<(), TuiError> {
    local(libgaki::ops::collections::add(name, &app_ids(ids)?))
}

/// Remove games from a collection.
#[cfg(feature = "collections")]
pub fn collection_remove(name: &str, ids: &[i32]) -> Result<(), TuiError> {
    local(libgaki::ops::collections::remove(name, &app_ids(ids)?))
}

/// Download Steam's collections and merge them into the local store.
#[cfg(feature = "collections")]
pub fn collections_pull() -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::collections::pull(&client).await
    })
    .map(drop)
}

/// Upload the local collections to Steam. This changes the Steam account;
/// the overlay key is the confirmation.
#[cfg(feature = "collections")]
pub fn collection_push() -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::collections::push(&client).await
    })
    .map(drop)
}

/// Pull, then push. This changes the Steam account.
#[cfg(feature = "collections")]
pub fn collection_sync() -> Result<(), TuiError> {
    in_process(async {
        let client = worker::global().session().authed().await?;
        libgaki::ops::collections::sync(&client).await
    })
    .map(drop)
}

/// Apply a change to one game's `games.toml` entry and return its settings.
fn update_game_config(app_id: i32, change: GameEntryChange) -> Result<GameSettings, TuiError> {
    let (entry, _) = in_process(libgaki::ops::config::update_game_entry(
        app_id as u32,
        change,
    ))?;
    let (_, profiles, _) =
        libgaki::ops::config::profiles().map_err(|e| TuiError::Problem(format!("{e:#}")))?;
    let game = entry
        .entry
        .unwrap_or_else(|| libgaki::config::Game::new(app_id as u32));
    Ok(GameSettings {
        app_id: entry.app_id,
        profile: game.profile,
        platform: game.platform.map(|p| p.as_str().to_owned()),
        steam: game.steam.map(|s| s.as_str().to_owned()),
        effective_profile: entry.profile,
        effective_steam: entry.steam.as_str().to_owned(),
        wrappers: entry.wrappers,
        game_wrappers: game.wrappers,
        profiles: profiles.into_iter().map(|p| p.profile.name).collect(),
    })
}

/// Show a game's settings.
pub fn config_game_show(app_id: i32) -> Result<GameSettings, TuiError> {
    update_game_config(app_id, Default::default())
}

fn set_or_clear<T>(value: Option<T>) -> Change<T> {
    value.map_or(Change::Clear, Change::Set)
}

/// Set a game's profile; `None` goes back to `default_profile`.
pub fn config_game_set_profile(app_id: i32, profile: Option<&str>) -> Result<(), TuiError> {
    update_game_config(
        app_id,
        GameEntryChange {
            profile: set_or_clear(profile.map(str::to_owned)),
            ..Default::default()
        },
    )
    .map(drop)
}

/// Set a game's platform, `"windows"` or `"linux"`; `None` is automatic.
pub fn config_game_set_platform(app_id: i32, platform: Option<&str>) -> Result<(), TuiError> {
    use libgaki::config::Platform;
    let platform = match platform {
        None => None,
        Some("windows") => Some(Platform::Windows),
        Some("linux") => Some(Platform::Linux),
        Some(other) => {
            return Err(TuiError::Problem(format!(
                "unknown platform '{other}' (expected windows or linux)"
            )));
        }
    };
    update_game_config(
        app_id,
        GameEntryChange {
            platform: set_or_clear(platform),
            ..Default::default()
        },
    )
    .map(drop)
}

/// Set a game's Steam integration, `"auto"`, `"host"` or `"none"`; `None`
/// uses the profile's.
pub fn config_game_set_steam(app_id: i32, steam: Option<&str>) -> Result<(), TuiError> {
    use libgaki::config::SteamIntegration;
    let steam = match steam {
        None => None,
        Some("auto") => Some(SteamIntegration::Auto),
        Some("host") => Some(SteamIntegration::Host),
        Some("none") => Some(SteamIntegration::None),
        Some(other) => {
            return Err(TuiError::Problem(format!(
                "unknown Steam integration '{other}' (expected auto, host or none)"
            )));
        }
    };
    update_game_config(
        app_id,
        GameEntryChange {
            steam: set_or_clear(steam),
            ..Default::default()
        },
    )
    .map(drop)
}

/// Launch a game with extra options and wait for it to exit, reporting each
/// stage (pre-launch update, launch, the build picked) in the status cell.
/// Intended to be run on a dedicated thread.
pub fn play_with(id: i32, opts: PlayOpts, status: Arc<Mutex<Option<GameStatus>>>) {
    use libgaki::core::models::DepotPlatform;
    use libgaki::ops::launch::{LaunchEvent, PlayRequest};
    set_status(&status, "launching...");
    let platform = match opts.platform.as_deref() {
        Some("linux") => Some(DepotPlatform::Linux),
        Some("windows") => Some(DepotPlatform::Windows),
        _ => None,
    };
    let request = PlayRequest {
        proton: opts.proton,
        steam: opts.steam,
        platform,
        no_update: opts.no_update,
    };
    let on_event = |event: LaunchEvent<'_>| match event {
        LaunchEvent::Updating { required: true } => set_status(&status, "required update..."),
        LaunchEvent::Updating { required: false } => set_status(&status, "updating..."),
        LaunchEvent::DrmUnverified { .. } => set_status(
            &status,
            "no Steam ticket; launching anyway (try Steam mode)",
        ),
        LaunchEvent::Launching => set_status(&status, "launching..."),
        LaunchEvent::Target { proton: Some(p) } => set_status(&status, &format!("running ({p})")),
        LaunchEvent::Target { proton: None } => set_status(&status, "running (native)"),
    };
    let outcome = worker::global().block_on(async {
        let mut client = worker::global().session().authed().await?;
        let game = libgaki::ops::library::find_game(&mut client, id as u32).await?;
        libgaki::ops::launch::play(&mut client, game, &request, &on_event, |rx| {
            drive_progress(rx, &status, ProgressLabel::Update)
        })
        .await
    });
    finish(&status, outcome, "ran (finished)");
}

#[cfg(test)]
mod tests;
