//! Library browsing model: the single source of truth for what the user is
//! looking at. Combines a tab filter (All/Installed/Updates/Favourites), a live
//! fuzzy text query, and a sort order, and owns the selection. Every browse
//! widget renders from this; the event loop only mutates it.
//!
//! The state lives in [`Browser`]; each submodule holds one view's methods.

mod account;
mod achievements;
mod actions;
#[cfg(feature = "versioning")]
mod branches;
#[cfg(feature = "social")]
mod chat;
#[cfg(feature = "cloud")]
mod cloud;
#[cfg(feature = "collections")]
mod collections;
pub mod depots;
mod dlc;
#[cfg(feature = "social")]
mod friends;
mod game_config;
mod help;
#[cfg(feature = "install")]
mod install;
#[cfg(feature = "inventory")]
mod inventory;
pub mod job;
mod launch;
mod launch_targets;
mod library;
mod links;
#[cfg(feature = "market")]
mod market;
mod proton;
#[cfg(feature = "install")]
mod relocate;
mod running;
#[cfg(feature = "store")]
pub mod store;
#[cfg(feature = "versioning")]
mod versions;
#[cfg(feature = "workshop")]
mod workshop;

pub use actions::{Action, ActionRow};
#[cfg(feature = "install")]
pub use install::InstallPhase;
pub use library::{Badge, Counts, Filter, Sort, View, badge};
pub use links::Link;

#[cfg(feature = "install")]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ratatui::widgets::ListState;

use crate::app::game::Game;
use crate::app::game::status::GameStatus;
use crate::app::steam::{self, Account, LauncherSettings};

/// A one-shot result slot a worker thread fills and the UI loop drains.
type Slot<T> = Arc<Mutex<Option<T>>>;

/// The browse state.
pub struct Browser {
    items: Vec<Game>,
    pub query: String,
    pub filter: Filter,
    pub sort: Sort,
    pub state: ListState,
    /// Whether the text filter is focused (typing edits the query).
    pub filtering: bool,
    /// Whether the help overlay is open.
    pub show_help: bool,
    /// Scroll offset (top row) within the help overlay.
    pub help_scroll: u16,
    /// Whether the Steam Cloud overlay is open.
    pub show_cloud: bool,
    /// Cloud files for the game the overlay is showing.
    pub cloud_files: Vec<steam::CloudFile>,
    /// A short status line for the cloud overlay (errors, sync progress).
    pub cloud_status: String,
    /// Whether the account overlay is open.
    pub show_account: bool,
    /// The fetched account details, shown by the account overlay.
    pub account_info: Option<Account>,
    /// Whether the config (settings) overlay is open.
    pub show_config: bool,
    /// The fetched launcher configuration, shown by the config overlay.
    pub config_info: Option<LauncherSettings>,
    /// The fetched network proxy setting, shown
    /// in the config overlay. Fetched alongside `config_info` when it opens.
    pub config_proxy: Option<steam::ProxySettings>,
    /// The inline proxy-URL edit buffer in the config overlay (None = not
    /// editing; Some = the URL the user is typing, empty to clear).
    pub config_proxy_input: Option<String>,
    /// Whether the Steam Wallet overlay is open.
    pub show_wallet: bool,
    /// The fetched wallet balance, shown by the wallet overlay.
    pub wallet_info: Option<steam::Wallet>,
    /// Whether the description panel is expanded beyond its collapsed cap.
    pub expand_description: bool,
    /// Whether the achievements overlay is open.
    pub show_achievements: bool,
    /// The selected game's achievements (loaded when the overlay opens).
    pub achievements: Vec<steam::Achievement>,
    /// Scroll offset (top row) within the achievements overlay.
    pub ach_scroll: usize,
    /// The active top-level view. `Library` shows the game list under the four
    /// filter tabs; `Friends` swaps the list for the Friends & Chat panel (the
    /// fifth tab) and gives it keyboard focus (so j/k move the highlight and
    /// c/Enter/t act on the selected friend).
    pub view: View,
    /// The logged-in user's friends (loaded lazily the first time the panel is
    /// focused).
    pub friends: Vec<steam::Friend>,
    /// The highlighted row within the Friends panel. The widget derives its own
    /// scroll window from this so the highlight is always visible.
    pub friends_index: usize,
    /// Whether the add-friend (search/add) overlay is open.
    pub show_friend_add: bool,
    /// The reference the user is typing into the add-friend overlay (SteamID64 /
    /// profile URL / vanity name).
    pub friend_add_query: String,
    /// A short status line for the add-friend overlay (search/add progress, the
    /// resolved account, or an error). Shared with the worker thread that runs
    /// the (off-UI-thread) `friends search`/`friends add` calls.
    pub friend_add_status: Arc<Mutex<String>>,
    /// The account resolved by the last `friends search`, shown as a preview and
    /// used as the confirmation target for `friends add`. Shared with the worker
    /// thread that fills it in.
    pub friend_search_result: Option<steam::FriendLookup>,
    /// Resolved account published by an in-flight `friends search`, adopted into
    /// `friend_search_result` on the next poll (off-UI-thread handoff). Tagged
    /// with the search generation it was launched for so a stale (already-edited)
    /// worker's result can be dropped instead of adopted.
    #[cfg(feature = "social")]
    friend_search_pending: Arc<Mutex<Option<(u64, steam::FriendLookup)>>>,
    /// Monotonic generation for the add-friend query. Bumped on every query edit
    /// and on overlay open/close; a search worker captures it at spawn and the
    /// poll only adopts a result whose tag still matches (closing the TOCTOU
    /// where a late worker could revive a preview for the edited query).
    #[cfg(feature = "social")]
    friend_search_gen: u64,
    /// Whether the remove-friend confirmation prompt is open for the selection.
    pub confirm_friend_remove: bool,
    /// Set true by a friend add/remove worker on success so the next poll
    /// re-fetches the friends roster off the UI thread.
    #[cfg(feature = "social")]
    friends_refresh_pending: Arc<Mutex<bool>>,
    /// A freshly fetched roster published by the refresh worker, adopted into
    /// `friends` on the next poll (off-UI-thread handoff).
    #[cfg(feature = "social")]
    friends_roster_pending: Arc<Mutex<Option<Vec<steam::Friend>>>>,
    /// Whether the roster has been fetched at least once. Distinguishes a
    /// genuinely empty friends list from a not-yet-loaded one, so the lazy load
    /// on first entering the Friends tab fires exactly once.
    #[cfg(feature = "social")]
    friends_loaded: bool,
    /// True while the initial (lazy) roster fetch is in flight, so the Friends
    /// panel can show "Loading friends…" instead of an empty/"no friends" state.
    pub friends_loading: bool,
    /// Whether the chat view is open.
    pub show_chat: bool,
    /// The messages in the open conversation (loaded when chat opens).
    pub chat_messages: Vec<steam::ChatMessage>,
    /// SteamID64 of the friend the chat view is talking to.
    pub chat_steamid: u64,
    /// Display name of the chat partner (shown in the title).
    pub chat_partner: String,
    /// The message the user is composing.
    pub chat_input: String,
    /// Scroll offset (top row) within the chat message list.
    pub chat_scroll: usize,
    /// Whether the inventory overlay is open.
    pub show_inventory: bool,
    /// The selected game's inventory items (loaded when the overlay opens).
    pub inventory: Vec<steam::InventoryItem>,
    /// Scroll offset (top row) within the inventory overlay.
    pub inv_scroll: usize,
    /// Whether the market listings overlay is open.
    pub show_market: bool,
    /// The account's active listings and buy orders (loaded when the overlay opens).
    pub market: Vec<steam::MarketListing>,
    /// Scroll offset (top row) within the market overlay.
    pub market_scroll: usize,
    /// Whether the Workshop overlay is open.
    pub show_workshop: bool,
    /// The selected game's Workshop items (loaded when the overlay opens).
    pub workshop: Vec<steam::WorkshopItem>,
    /// Scroll offset (top row) within the Workshop overlay.
    pub workshop_scroll: usize,
    /// The app id the Workshop overlay is showing (used for browse/subscribe).
    #[cfg(feature = "workshop")]
    workshop_app_id: i32,
    /// Whether the overlay is in browse/search mode (vs. the subscribed list).
    pub workshop_browse: bool,
    /// The Workshop browse/search query the user is typing.
    pub workshop_query: String,
    /// Browse/search results (the current page from `workshop browse`).
    pub workshop_results: Vec<steam::WorkshopItem>,
    /// The highlighted row within the browse results list.
    pub workshop_index: usize,
    /// True while a browse/search request is in flight (drives a spinner line).
    pub workshop_searching: bool,
    /// A transient status line for the browse pane (subscribe/unsubscribe/error).
    pub workshop_status: String,
    /// Generation tag for browse requests: a worker's result is applied only if
    /// its tag still matches, so a slow earlier search never clobbers a newer one.
    #[cfg(feature = "workshop")]
    workshop_gen: u64,
    /// Slot a browse worker posts its `(generation, result)` into; polled each
    /// loop iteration by `poll_workshop`.
    #[cfg(feature = "workshop")]
    workshop_slot: Slot<(u64, Result<Vec<steam::WorkshopItem>, String>)>,
    /// True while a subscribe/unsubscribe request is in flight.
    pub workshop_acting: bool,
    /// Slot a subscribe/unsubscribe worker posts its outcome into:
    /// `(item_id, want_subscribed, Result<(), error>)`. Polled by `poll_workshop`.
    #[cfg(feature = "workshop")]
    workshop_action_slot: Slot<(u64, bool, Result<(), String>)>,
    /// Slot a subscribe/unsubscribe worker posts a re-fetched subscribed list
    /// into on success: `(generation, items)`. Drained (non-blocking) by
    /// `poll_workshop` and installed only if the generation still matches, so a
    /// stale refresh can never clobber newer overlay state. This keeps the
    /// post-action `workshop list` shell-out entirely off the render thread.
    #[cfg(feature = "workshop")]
    workshop_refresh_slot: Slot<(u64, Vec<steam::WorkshopItem>)>,
    /// True while a rate (thumbs up/down) request is in flight.
    pub workshop_rating: bool,
    /// Slot a rate worker posts its outcome into: `(generation, Result<(), error>)`.
    /// Drained by `poll_workshop`; a stale-generation result (the overlay closed
    /// or moved on) is dropped rather than shown.
    #[cfg(feature = "workshop")]
    workshop_rate_slot: Slot<(u64, Result<(), String>)>,
    /// Whether the comments sub-pane is open (over the browse results).
    pub workshop_comments_open: bool,
    /// True while a comments fetch is in flight (drives a spinner line).
    pub workshop_comments_loading: bool,
    /// The fetched comments for the item the sub-pane is showing.
    pub workshop_comments: Vec<steam::WorkshopComment>,
    /// Scroll offset (top row) within the comments sub-pane.
    pub workshop_comments_scroll: usize,
    /// A transient status line for the comments sub-pane (errors / empty).
    pub workshop_comments_status: String,
    /// The id of the item the comments sub-pane was opened for (shown in title).
    #[cfg(feature = "workshop")]
    workshop_comments_id: u64,
    /// Slot a comments worker posts its `(generation, result)` into; drained by
    /// `poll_workshop` and applied only if the generation still matches, so a
    /// slow fetch for a since-closed/superseded sub-pane is discarded.
    #[cfg(feature = "workshop")]
    workshop_comments_slot: Slot<(u64, Result<Vec<steam::WorkshopComment>, String>)>,
    /// Whether the uninstall confirmation prompt is open for the selection.
    pub confirm_uninstall: bool,
    /// Whether the DLC overlay is open.
    pub show_dlc: bool,
    /// DLC for the game the overlay was opened for.
    pub dlc: Vec<steam::Dlc>,
    /// The highlighted row within the DLC overlay.
    pub dlc_index: usize,
    /// The base app id the DLC overlay is showing (for re-fetching after a toggle).
    dlc_app_id: i32,
    /// Whether the branches overlay is open.
    pub show_branches: bool,
    /// Beta branches for the game the overlay was opened for.
    pub branches: Vec<steam::Branch>,
    /// The highlighted row within the branches overlay.
    pub branch_index: usize,
    /// The app id the branches overlay is showing (used when switching branch).
    #[cfg(feature = "versioning")]
    branch_app_id: i32,
    /// Whether the depots overlay is open.
    pub show_depots: bool,
    /// The depots overlay's list (and, with `depot`, its file view).
    pub depots: depots::DepotsView,
    /// The Store overlay (`Y`), when open.
    #[cfg(feature = "store")]
    pub store: Option<store::StoreView>,
    /// Whether the launch-options overlay is open.
    pub show_launch: bool,
    /// The selected game's launch options (loaded when the overlay opens).
    pub launch_options: Vec<steam::LaunchOption>,
    /// Scroll offset (top row) within the launch-options overlay.
    pub launch_scroll: usize,
    /// Whether the move (relocate install) prompt is open.
    pub show_move: bool,
    /// The destination library path the user is typing.
    pub move_path: String,
    /// The app id being moved.
    pub move_app_id: i32,
    /// A short status line for the move prompt (progress, errors).
    pub move_status: String,
    /// Whether the relink (relink install) prompt is open.
    pub show_relink: bool,
    /// The destination library path the user is typing.
    pub relink_path: String,
    /// The app id being relinked.
    pub relink_app_id: i32,
    /// A short status line for the relink prompt (progress, errors).
    pub relink_status: String,
    /// Whether the import (register existing install) prompt is open.
    pub show_import: bool,
    /// The library path the user is typing.
    pub import_path: String,
    /// The app id being imported.
    pub import_app_id: i32,
    /// A short status line for the import prompt (progress, errors).
    pub import_status: String,
    /// Whether the Proton runtimes overlay is open.
    pub show_proton: bool,
    /// The Proton/Wine runtimes (loaded when the overlay opens).
    pub protons: Vec<steam::ProtonRuntime>,
    /// The highlighted row within the Proton overlay.
    pub proton_index: usize,
    /// Shared status cell for an in-flight Proton install/uninstall, streamed
    /// from the backend worker thread (mirrors a game's install status).
    pub proton_status: Arc<Mutex<Option<GameStatus>>>,
    /// Whether the Proton uninstall confirmation prompt is showing.
    pub confirm_proton_uninstall: bool,
    /// Whether the running-games overlay is open.
    pub show_running: bool,
    /// The games gaki currently has running (loaded when the overlay opens).
    pub running: Vec<steam::RunningGame>,
    /// The highlighted row within the running overlay.
    pub running_index: usize,
    /// Whether the Community Market search overlay is open.
    pub show_market_search: bool,
    /// The query the user is typing into the market search overlay.
    pub market_query: String,
    /// The resolved search results (adopted from the worker cell on each poll).
    pub market_results: Vec<steam::MarketSearchResult>,
    /// The highlighted row within the search results.
    pub market_results_index: usize,
    /// Scroll offset (top row) within the search-results list.
    pub market_results_scroll: usize,
    /// A short status line for the search overlay (progress, errors, price).
    pub market_search_status: String,
    /// Generation tag for market searches. Bumped whenever the query changes or
    /// the search overlay opens/closes; captured at worker spawn and re-checked
    /// in [`Browser::poll_market`] so a slow OLDER search can't clobber a NEWER
    /// one's results when Enter is hit in rapid succession (mirrors
    /// `friend_search_gen`).
    #[cfg(feature = "market")]
    market_search_gen: u64,
    /// Shared cell the background search worker publishes its (gen-tagged) result
    /// into.
    #[cfg(feature = "market")]
    market_search_cell: Arc<Mutex<(u64, steam::MarketSearchState)>>,
    /// Shared cell the background price worker publishes its result into.
    #[cfg(feature = "market")]
    market_price_cell: Arc<Mutex<steam::MarketPriceState>>,
    /// Per-game install controls, keyed by app id. An entry is created when the
    /// UI starts an install and lets it pause/stop the in-flight download.
    #[cfg(feature = "install")]
    install_controls: HashMap<i32, steam::InstallControl>,
    /// The library folder each in-flight install was started in, keyed by app
    /// id, so a paused install resumes into the *same* library (not the default).
    #[cfg(feature = "install")]
    install_library_choice: HashMap<i32, Option<String>>,
    /// Whether the "game not installed — install now?" prompt is open (shown
    /// when the user tries to launch a game the listing marks not installed).
    pub confirm_install: bool,
    /// Whether the install-location picker (choose which Steam library folder to
    /// install into) is open.
    pub show_install_picker: bool,
    /// The available library folders shown in the picker, each with its
    /// drive's free space.
    pub install_libraries: Vec<steam::LibraryFolder>,
    /// Estimated on-disk size of the game being installed, for the picker to
    /// show and to gauge whether a library has room. `None` if unknown.
    pub install_estimate: Option<u64>,
    /// The highlighted row within the install picker.
    pub install_picker_index: usize,
    /// Off-thread inbox for targeted single-game install-state refreshes (after
    /// an install completes or is cancelled). A worker fetches the fresh `list`
    /// entry and drops it here; [`Browser::poll_game_refresh`] adopts it,
    /// patching just that game's `installed`/update flags — no full reload.
    #[cfg(feature = "install")]
    game_refresh_slot: Arc<Mutex<Vec<steam::LibraryGame>>>,
    /// The running background update check, if any. Its worker fills the slot
    /// with the app ids that have an update, or the error;
    /// [`Browser::poll_update_check`] adopts it.
    #[cfg(feature = "install")]
    update_check: Option<install::UpdateSlot>,
    /// A transient one-line notice (e.g. an action that failed) shown in the
    /// status bar until the next keypress. Keeps a failed backend call from
    /// having to crash the whole TUI to report itself.
    pub notice: Option<String>,
    /// Why the last [`Link`] didn't open, filled by its thread.
    link_error: Slot<String>,

    // --- Actions menu (per-game command palette) ---
    /// Whether the per-game Actions menu is open. This is the primary way to
    /// reach the many per-game actions without memorising 40 hotkeys; the base
    /// keymap keeps single-key accelerators for power users.
    pub show_actions: bool,
    /// The highlighted row within the *filtered* Actions menu.
    pub actions_index: usize,
    /// The live filter query typed into the Actions menu (empty = show all).
    pub actions_filter: String,
    /// The app id the Actions menu was opened for.
    actions_app_id: i32,

    // --- Versions & pinning overlay ---
    /// Whether the versions/pinning overlay is open.
    pub show_versions: bool,
    /// Per-depot current manifest ids for the game.
    pub versions_manifests: Vec<steam::DepotManifestInfo>,
    /// Install + pin state for the game: the pin flag
    /// and the pinned depot→manifest map.
    pub versions_available: Option<steam::PinState>,
    /// The highlighted depot row within the versions overlay.
    pub versions_index: usize,
    /// The app id the versions overlay is showing.
    #[cfg(feature = "versioning")]
    versions_app_id: i32,
    /// A transient status line for the versions overlay (pin/unpin/downgrade).
    pub versions_status: String,
    /// When `Some`, the overlay is prompting for a manifest id to downgrade the
    /// highlighted depot to (older ids come from SteamDB); the buffer is what the
    /// user has typed so far.
    pub versions_input: Option<String>,
    /// Shared status cell for an in-flight downgrade, streamed from the backend
    /// worker thread (mirrors a game's install status).
    pub versions_busy: Arc<Mutex<Option<GameStatus>>>,

    // --- Game settings overlay (per-game config overrides) ---
    /// Whether the per-game settings overlay is open.
    pub show_game_config: bool,
    /// The game's current config overrides.
    pub game_config: Option<steam::GameSettings>,
    /// The highlighted settings row.
    pub game_config_index: usize,
    /// The app id the settings overlay is showing.
    game_config_app_id: i32,
    /// A transient status line for the settings overlay.
    pub game_config_status: String,

    // --- Collections overlay ---
    /// Whether the collections overlay is open.
    pub show_collections: bool,
    /// The account's Steam library collections.
    pub collections: Vec<steam::Collection>,
    /// The highlighted collection row.
    pub collections_index: usize,
    /// A transient status line for the collections overlay.
    pub collections_status: String,
    /// The game the overlay adds to / removes from the highlighted collection.
    #[cfg(feature = "collections")]
    collections_app_id: i32,
    /// The inline text-entry buffer for creating a collection (None = not
    /// prompting).
    pub collections_input: Option<String>,

    // --- Launch targets overlay ("Play as…") ---
    /// Whether the launch-targets overlay is open.
    pub show_launch_targets: bool,
    /// The game's launch targets.
    pub launch_targets: Vec<steam::LaunchTarget>,
    /// The highlighted target row.
    pub launch_targets_index: usize,
    /// A transient status line (fetch error, unavailable target).
    pub launch_targets_status: String,
}

impl Browser {
    pub fn new(items: Vec<Game>) -> Browser {
        let mut browser = Browser {
            items,
            query: String::new(),
            filter: Filter::All,
            sort: Sort::Name,
            state: ListState::default(),
            filtering: false,
            show_help: false,
            help_scroll: 0,
            show_cloud: false,
            cloud_files: Vec::new(),
            cloud_status: String::new(),
            show_account: false,
            account_info: None,
            show_config: false,
            config_info: None,
            config_proxy: None,
            config_proxy_input: None,
            show_wallet: false,
            wallet_info: None,
            expand_description: false,
            show_achievements: false,
            achievements: Vec::new(),
            ach_scroll: 0,
            view: View::Library,
            friends: Vec::new(),
            friends_index: 0,
            show_friend_add: false,
            friend_add_query: String::new(),
            friend_add_status: Arc::new(Mutex::new(String::new())),
            friend_search_result: None,
            #[cfg(feature = "social")]
            friend_search_pending: Arc::new(Mutex::new(None)),
            #[cfg(feature = "social")]
            friend_search_gen: 0,
            confirm_friend_remove: false,
            #[cfg(feature = "social")]
            friends_refresh_pending: Arc::new(Mutex::new(false)),
            #[cfg(feature = "social")]
            friends_roster_pending: Arc::new(Mutex::new(None)),
            #[cfg(feature = "social")]
            friends_loaded: false,
            friends_loading: false,
            show_chat: false,
            chat_messages: Vec::new(),
            chat_steamid: 0,
            chat_partner: String::new(),
            chat_input: String::new(),
            chat_scroll: 0,
            show_inventory: false,
            inventory: Vec::new(),
            inv_scroll: 0,
            show_market: false,
            market: Vec::new(),
            market_scroll: 0,
            show_workshop: false,
            workshop: Vec::new(),
            workshop_scroll: 0,
            #[cfg(feature = "workshop")]
            workshop_app_id: 0,
            workshop_browse: false,
            workshop_query: String::new(),
            workshop_results: Vec::new(),
            workshop_index: 0,
            workshop_searching: false,
            workshop_status: String::new(),
            #[cfg(feature = "workshop")]
            workshop_gen: 0,
            #[cfg(feature = "workshop")]
            workshop_slot: Arc::new(Mutex::new(None)),
            workshop_acting: false,
            #[cfg(feature = "workshop")]
            workshop_action_slot: Arc::new(Mutex::new(None)),
            #[cfg(feature = "workshop")]
            workshop_refresh_slot: Arc::new(Mutex::new(None)),
            workshop_rating: false,
            #[cfg(feature = "workshop")]
            workshop_rate_slot: Arc::new(Mutex::new(None)),
            workshop_comments_open: false,
            workshop_comments_loading: false,
            workshop_comments: Vec::new(),
            workshop_comments_scroll: 0,
            workshop_comments_status: String::new(),
            #[cfg(feature = "workshop")]
            workshop_comments_id: 0,
            #[cfg(feature = "workshop")]
            workshop_comments_slot: Arc::new(Mutex::new(None)),
            confirm_uninstall: false,
            show_dlc: false,
            dlc: Vec::new(),
            dlc_index: 0,
            dlc_app_id: 0,
            show_branches: false,
            branches: Vec::new(),
            branch_index: 0,
            #[cfg(feature = "versioning")]
            branch_app_id: 0,
            show_depots: false,
            depots: depots::DepotsView::default(),
            #[cfg(feature = "store")]
            store: None,
            show_launch: false,
            launch_options: Vec::new(),
            launch_scroll: 0,
            show_move: false,
            move_path: String::new(),
            move_app_id: 0,
            move_status: String::new(),
            show_relink: false,
            relink_path: String::new(),
            relink_app_id: 0,
            relink_status: String::new(),
            show_import: false,
            import_path: String::new(),
            import_app_id: 0,
            import_status: String::new(),
            show_proton: false,
            protons: Vec::new(),
            proton_index: 0,
            proton_status: Arc::new(Mutex::new(None)),
            confirm_proton_uninstall: false,
            show_running: false,
            running: Vec::new(),
            running_index: 0,
            show_market_search: false,
            market_query: String::new(),
            market_results: Vec::new(),
            market_results_index: 0,
            market_results_scroll: 0,
            market_search_status: String::new(),
            #[cfg(feature = "market")]
            market_search_gen: 0,
            #[cfg(feature = "market")]
            market_search_cell: Arc::new(Mutex::new((0, steam::MarketSearchState::Idle))),
            #[cfg(feature = "market")]
            market_price_cell: Arc::new(Mutex::new(steam::MarketPriceState::Idle)),
            #[cfg(feature = "install")]
            install_controls: HashMap::new(),
            #[cfg(feature = "install")]
            install_library_choice: HashMap::new(),
            confirm_install: false,
            show_install_picker: false,
            install_libraries: Vec::new(),
            install_estimate: None,
            install_picker_index: 0,
            #[cfg(feature = "install")]
            game_refresh_slot: Arc::new(Mutex::new(Vec::new())),
            #[cfg(feature = "install")]
            update_check: None,
            notice: None,
            link_error: Arc::new(Mutex::new(None)),
            show_actions: false,
            actions_index: 0,
            actions_filter: String::new(),
            actions_app_id: 0,
            show_versions: false,
            versions_manifests: Vec::new(),
            versions_available: None,
            versions_index: 0,
            #[cfg(feature = "versioning")]
            versions_app_id: 0,
            versions_status: String::new(),
            versions_input: None,
            versions_busy: Arc::new(Mutex::new(None)),
            show_game_config: false,
            game_config: None,
            game_config_index: 0,
            game_config_app_id: 0,
            game_config_status: String::new(),
            show_collections: false,
            collections: Vec::new(),
            collections_index: 0,
            collections_status: String::new(),
            #[cfg(feature = "collections")]
            collections_app_id: 0,
            collections_input: None,
            show_launch_targets: false,
            launch_targets: Vec::new(),
            launch_targets_index: 0,
            launch_targets_status: String::new(),
        };
        browser.reset_selection();
        browser
    }
}
