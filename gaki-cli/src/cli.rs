//! clap CLI type definitions (parsed command surface).

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use libgaki::core::models::DepotPlatform;

/// Parses a game reference: an app id, or a game's `name` alias from
/// `games.toml`.
pub(crate) fn app_ref(reference: &str) -> Result<u32, String> {
    libgaki::config::resolve_app_ref(reference)
}

/// The `--config-dir` value in `args`, found before clap runs: resolving a
/// game alias while parsing needs the config directory already set.
pub(crate) fn config_dir_arg<I: IntoIterator<Item = String>>(args: I) -> Option<PathBuf> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if arg == "--config-dir" {
            return args.next().map(PathBuf::from);
        }
        if let Some(dir) = arg.strip_prefix("--config-dir=") {
            return Some(PathBuf::from(dir));
        }
    }
    None
}

/// gaki — a command-line Steam launcher (auth, library, install, launch).
/// `--help` footer of the chat commands (STEAM-RULES.md, "Your part").
#[cfg(feature = "social")]
const RULES_CHAT: &str = "Steam's rules forbid bots: a script that sends or answers messages \
for you is automation under the Steam Subscriber Agreement (\u{a7}4.C). See docs/STEAM-RULES.md.";

/// `--help` footer of the inventory, wallet and market commands.
#[cfg(any(feature = "inventory", feature = "market"))]
const RULES_MARKET: &str = "Read-only: gaki never buys, sells or trades. Steam limits these pages \
hard, so answers are reused for a while (prices 5 minutes, your own data 1 minute) and requests \
are spaced 3 seconds apart. Scripts that poll the market are automation under the Steam \
Subscriber Agreement (\u{a7}4.C). See docs/STEAM-RULES.md.";

#[derive(Parser)]
#[command(
    name = env!("CARGO_BIN_NAME"),
    version,
    about,
    long_about = None,
    // Default template, with a "Version: x.y.z" line inserted under the about text.
    help_template = "{before-help}{about-with-newline}Version: {version}\n\n{usage-heading} {usage}\n\n{all-args}{after-help}"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
    /// Emit output (and errors) as JSON. Works with every command.
    #[arg(short = 'j', long, global = true)]
    pub(crate) json: bool,
    /// Increase log verbosity (repeatable: -v, -vv, -vvv). Without it only
    /// warnings and errors are logged. -v adds progress/debug logs and a launched
    /// game's own output; -vv/-vvv unmute the Steam networking stack so a stalled
    /// command shows where it is stuck. `--log` overrides the logs.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub(crate) verbose: u8,
    /// Log filter directives (e.g. `debug,gaki_steam_vent=info`), overriding `-v`.
    #[arg(long, global = true, value_name = "FILTER", value_parser = libgaki::infra::logging::validate_filter)]
    pub(crate) log: Option<String>,
    /// Use this config directory instead of `~/.config/gaki`. A daemon serves
    /// one config directory, so pair it with `--socket` or `--no-daemon`.
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) config_dir: Option<std::path::PathBuf>,
    /// Run the command in this process, never through the session daemon.
    /// Accepted (and hidden) in builds without one, so scripts can always pass it.
    #[arg(long, global = true, hide = cfg!(not(feature = "authentication")))]
    #[cfg_attr(not(feature = "authentication"), allow(dead_code))]
    pub(crate) no_daemon: bool,
    /// Use a running session daemon, but never start one (for drivers that
    /// manage the daemon themselves, or whose spawn chains wait on children).
    #[arg(
        long,
        global = true,
        conflicts_with = "no_daemon",
        hide = cfg!(not(feature = "authentication"))
    )]
    #[cfg_attr(not(feature = "authentication"), allow(dead_code))]
    pub(crate) no_spawn: bool,
    /// Session daemon socket path, to run or reach an isolated daemon.
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        hide = cfg!(not(feature = "authentication"))
    )]
    #[cfg_attr(not(feature = "authentication"), allow(dead_code))]
    pub(crate) socket: Option<String>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Authenticate with Steam and persist the session.
    #[cfg(feature = "authentication")]
    Login(LoginArgs),
    /// Describe this build for drivers: commands, flags, JSON events, error types
    /// and exit codes, and the `--json` schema version. Offline. Without `--json` it
    /// prints a PowerShell-style list and error table. Name a command path
    /// (e.g. `capabilities config game`) for its flags, values and help.
    Capabilities {
        /// Command path to describe, e.g. `config game`.
        path: Vec<String>,
    },
    /// List games in your library.
    #[cfg(feature = "library")]
    List {
        /// Only show installed games.
        #[arg(short, long)]
        installed: bool,
        /// Filter by case-insensitive substring of the game name.
        #[arg(short, long)]
        search: Option<String>,
        /// Only show games in the named collection (by name or id). Static
        /// collections only — dynamic (filter-based) ones can't be resolved offline.
        #[arg(short = 'c', long)]
        collection: Option<String>,
        /// Show an ONLINE column indicating whether each game appears to require
        /// an online connection (inferred from Steam store categories). This
        /// fetches PICS appinfo per game, so it is slower than a plain listing.
        #[arg(short = 'o', long)]
        online: bool,
        /// Compute `update_available` for installed games by comparing local and
        /// remote depot manifests (applies to owned *and* Family-Shared games).
        /// Requires a connection and a manifest fetch per game, so it is slower
        /// than a plain listing; off by default.
        #[arg(short = 'u', long)]
        check_updates: bool,
        /// Order of the listed games.
        #[arg(long, value_enum, default_value_t = ListSort::Name)]
        sort: ListSort,
    },
    /// Show detailed information about a game.
    #[cfg(feature = "library")]
    Info {
        /// One or more app ids. Multiple ids are fetched over a *single* Steam
        /// logon (one batched StoreBrowse call) — far cheaper than running `info`
        /// once per id. With `--json`, one id yields an object and several yield an
        /// array.
        #[arg(value_parser = app_ref, required = true)]
        app_ids: Vec<u32>,
        /// Also show storefront-only fields that have no CM-protocol source:
        /// system requirements, Metacritic, website and store genres/categories.
        /// One extra HTTPS request per app.
        #[arg(short = 'e', long)]
        extended: bool,
        /// Bypass the local metadata cache and fetch fresh data from Steam.
        /// By default `info` serves cached store metadata (TTL via
        /// `config info-cache-ttl`, default 6h) to avoid a Steam logon per call.
        #[arg(short = 'n', long)]
        no_cache: bool,
        /// Price country code; defaults to US.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
    },
    /// Show a game's price, optionally across regions.
    #[cfg(feature = "store")]
    Price {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Country codes to compare, comma-separated.
        #[arg(short = 'r', long, value_delimiter = ',')]
        compare: Vec<String>,
        /// Country code for a single-region lookup.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
    },
    /// Search the store by title.
    #[cfg(feature = "store")]
    Search {
        term: String,
        /// Maximum results.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
        /// Country code for prices.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
    },
    /// Discounted and top-selling games in a region.
    #[cfg(feature = "store")]
    Deals {
        /// Which slice of the store to list.
        #[arg(short = 's', long, value_enum, default_value_t = DealsScopeArg::DiscountedTopSellers)]
        scope: DealsScopeArg,
        /// Minimum discount percent for specials.
        #[arg(short = 'm', long, default_value_t = 1)]
        min_discount: i32,
        /// Maximum results.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: i32,
        /// Result offset for paging.
        #[arg(short = 'o', long, default_value_t = 0)]
        start: i32,
        /// Country code for prices.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
    },
    /// Current player count for an app.
    #[cfg(feature = "store")]
    Players {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Store-wide sales and events currently running.
    #[cfg(feature = "store")]
    Events {
        /// Country code the events apply to.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
    },
    /// Recent news (storefront web; no CM equivalent).
    #[cfg(feature = "store")]
    News {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Maximum items.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: u32,
        /// Characters of body text to keep.
        #[arg(short = 'm', long, default_value_t = 300)]
        max_length: u32,
    },
    /// User reviews, paged (storefront web, no CM equivalent).
    #[cfg(feature = "store")]
    Reviews(ReviewsArgs),
    /// Profile by SteamID64, /profiles/ URL, friend name, `me`.
    #[cfg(feature = "social")]
    User { user: String },
    /// Your wishlist, or a friend's.
    #[command(args_conflicts_with_subcommands = true)]
    #[cfg(feature = "store")]
    Wishlist {
        /// Whose wishlist; defaults to yours.
        user: Option<String>,
        /// Maximum items.
        #[arg(short = 'n', long, default_value_t = 50)]
        count: usize,
        /// Skip this many items first.
        #[arg(short = 'o', long, default_value_t = 0)]
        offset: usize,
        /// Country code for prices.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
        #[command(subcommand)]
        command: Option<WishlistCommand>,
    },
    /// Games similar to a given app.
    #[cfg(feature = "store")]
    Similar {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Maximum results.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: i32,
        /// Country code for prices.
        #[arg(short = 'c', long, alias = "cc")]
        country: Option<String>,
    },
    /// Download and install a game.
    #[cfg(feature = "install")]
    Install(InstallArgs),
    /// List the Steam library folders games can be installed into (one per
    /// drive/location). Use `--json` for tooling.
    #[cfg(feature = "library")]
    Libraries,
    /// Export the library to a TOML file, or install the games listed in one.
    #[cfg(feature = "library")]
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// List installed games with an update available, or update one by app id.
    #[cfg(feature = "install")]
    Update {
        /// Game to update. Omit to list every installed game that needs an update.
        #[arg(value_parser = app_ref)]
        app_id: Option<u32>,
        /// Update even if the game is pinned (see `gaki-cli pin`), overriding the lock.
        #[arg(short = 'f', long)]
        force: bool,
    },
    /// Launch a game and wait for it to exit.
    #[cfg(feature = "library")]
    Play {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Force a specific Proton/Wine runner (implies Windows target).
        #[arg(short, long)]
        proton: Option<String>,
        /// Run with real Steam integration: bridge to the host Steam client
        /// (started silently if not running) so Steamworks online features work.
        /// Same as `steam = "host"` in `games.toml` or the profile, which it
        /// overrides. Implied for Family-Shared games, which require it.
        #[arg(short = 's', long)]
        steam: bool,
        /// Run this installed build (`linux` native or `windows` via Proton),
        /// overriding `games.toml` and `default_platform` for this launch. See
        /// `gaki-cli launch-targets`.
        #[arg(short = 't', long)]
        platform: Option<PlatformArg>,
        /// Skip the automatic update check and install before launching.
        #[arg(short = 'N', long = "no-update", alias = "noupdate")]
        no_update: bool,
    },
    /// List the ways an installed game can be launched: each installed build
    /// (native Linux / Windows) and, for Windows, each available Steam mode, with
    /// the `play` flags selecting it and which one plain `play` would pick.
    #[cfg(feature = "library")]
    LaunchTargets {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Compute the default as for `play --steam`.
        #[arg(short = 's', long)]
        steam: bool,
    },
    /// Stop a running game previously launched with `gaki-cli play`.
    #[cfg(feature = "library")]
    Stop {
        /// App id to stop. Omit to list the games gaki is tracking as running.
        #[arg(value_parser = app_ref)]
        app_id: Option<u32>,
        /// Force-kill the game immediately (SIGKILL) instead of asking it to exit
        /// gracefully first. Use when a game is hung and ignores a normal stop.
        #[arg(short = 'f', long)]
        force: bool,
    },
    /// List the games gaki is currently running.
    #[cfg(feature = "library")]
    Running,
    /// Delete gaki's caches and leftovers: art, `info`, library and TUI
    /// caches, old launch logs (each game's newest stays), unused install
    /// locks, and the shader caches of games that aren't installed. Lists them
    /// with their sizes and asks first. Subset flags pick kinds; without any,
    /// all of them. Never touches the config, the session, installs, saves,
    /// prefixes or partial downloads.
    Clean {
        /// Cover art and artwork.
        #[arg(long)]
        images: bool,
        /// Cached `info` records and store tag/category names.
        #[arg(long)]
        info: bool,
        /// The cached list of owned games.
        #[arg(long)]
        library: bool,
        /// gaki-tui's cached library and cover art.
        #[arg(long)]
        tui: bool,
        /// Launch logs, except each game's newest.
        #[arg(long)]
        logs: bool,
        /// Install lock files no process holds.
        #[arg(long)]
        locks: bool,
        /// Shader caches of games that aren't installed.
        #[arg(long)]
        shaders: bool,
        /// Every game's shader cache, installed ones too (they rebuild on the
        /// next launch).
        #[arg(long)]
        all_shaders: bool,
        /// Only list what would be deleted.
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Don't ask.
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Uninstall a game.
    #[cfg(feature = "install")]
    Uninstall {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Also delete the game's Wine prefix / compat data.
        #[arg(short = 'd', long)]
        delete_prefix: bool,
    },
    /// Verify the integrity of an installed game.
    #[cfg(feature = "install")]
    Verify {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// List games installed in more than one Steam library.
    ///
    /// A duplicate leaves two appmanifests disagreeing about what is installed,
    /// which makes the game report an available update on every check. Prints the
    /// redundant copies and the commands to remove them; deletes nothing itself.
    #[cfg(feature = "install")]
    Duplicates,
    /// Report whether a game is installed and its files are present on disk.
    #[cfg(feature = "install")]
    Available {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Verify a game's DRM tickets headlessly.
    #[cfg(feature = "library")]
    Drm {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Show account details for the logged-in user.
    #[cfg(feature = "library")]
    Account,
    /// List a game's DLC.
    #[cfg(feature = "library")]
    Dlc {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Enable an installed DLC for its base game.
    #[cfg(feature = "library")]
    Enable {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Stop Steam while applying the change, then restart it, so the running
        /// client picks it up. Steam reads DLC state only at startup.
        #[arg(short = 'r', long)]
        restart_steam: bool,
    },
    /// Disable a DLC for its base game.
    #[cfg(feature = "library")]
    Disable {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Stop Steam while applying the change, then restart it.
        #[arg(short = 'r', long)]
        restart_steam: bool,
    },
    /// Show the logged-in user's achievements for a game.
    #[cfg(feature = "library")]
    Achievements {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Download a game's cover/header artwork to the local image cache.
    #[cfg(feature = "library")]
    Image {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Write the image to this path instead of the cache directory.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Re-download even if a cached copy already exists.
        #[arg(short, long)]
        force: bool,
        /// List every store asset URL instead of downloading.
        #[arg(short = 'l', long)]
        list: bool,
        /// With `--list`: HEAD-check each URL.
        #[arg(short = 'p', long, requires = "list")]
        probe: bool,
    },
    /// Dump raw PICS appinfo sections (maintainer tool).
    #[command(hide = true)]
    #[cfg(feature = "library")]
    Appinfo {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Dump the store tag vocabulary (maintainer tool).
    #[command(hide = true)]
    #[cfg(feature = "library")]
    Tags {
        /// Print Rust source for `tags_table.rs`.
        #[arg(short = 'd', long)]
        dump: bool,
    },
    /// Move an installed game to a different Steam library folder
    #[cfg(feature = "install")]
    Move {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Destination Steam library folder (its root, containing `steamapps/`),
        /// e.g. `D:\SteamLibrary`. Must already be a Steam library.
        library: PathBuf,
        /// Stop Steam for the duration of the move and restart it afterward.
        /// Steam overwrites its data files on exit, so moving while it runs is
        /// unsafe; without this, the move refuses to run while Steam is open.
        #[arg(short = 'r', long)]
        restart_steam: bool,
    },
    /// Relink an install to a different Steam library.
    #[cfg(feature = "install")]
    Relink {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Destination Steam library root (containing `steamapps/`).
        library: PathBuf,
        /// Stop Steam for the duration and restart it afterward.
        #[arg(short = 'r', long)]
        restart_steam: bool,
    },
    /// Register an existing on-disk install with Steam.
    #[cfg(feature = "install")]
    Import {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Steam library root whose `steamapps/common/<installdir>` holds the files.
        library: PathBuf,
        /// Depot platform whose files are present. Defaults to the current OS.
        #[arg(short, long)]
        platform: Option<PlatformArg>,
        /// Stop Steam for the duration and restart it afterward.
        #[arg(short = 'r', long)]
        restart_steam: bool,
    },
    /// List available beta branches for a game.
    #[cfg(feature = "versioning")]
    Branches {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Switch a game to a different branch.
    #[cfg(feature = "versioning")]
    SetBranch {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        branch: String,
    },
    /// List depots for a game, or browse and download a depot's files.
    #[cfg(feature = "library")]
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Depots {
        #[arg(value_parser = app_ref, required = true)]
        app_id: Option<u32>,
        #[cfg(feature = "depot")]
        #[command(subcommand)]
        command: Option<DepotsCommand>,
    },
    /// List each depot's current manifest id per branch (version discovery for
    /// `downgrade`). Steam only exposes current ids — older ones live on SteamDB.
    #[cfg(feature = "versioning")]
    Manifests {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Only show this depot.
        #[arg(short = 'd', long)]
        depot: Option<u32>,
    },
    /// Install a specific (usually older) depot manifest and pin it — a downgrade.
    #[cfg(feature = "versioning")]
    Downgrade(DowngradeArgs),
    /// Pin a game to its currently-installed manifests, locking gaki's updates.
    #[cfg(feature = "versioning")]
    Pin {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Remove a game's version pin (unlock gaki's updates).
    #[cfg(feature = "versioning")]
    Unpin {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// List a game's launch options.
    #[cfg(feature = "library")]
    LaunchOptions {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Manage Steam Cloud saves for a game.
    #[cfg(feature = "cloud")]
    Cloud {
        #[command(subcommand)]
        command: CloudCommand,
    },
    /// Manage Steam Workshop items for a game.
    #[cfg(feature = "workshop")]
    Workshop {
        #[command(subcommand)]
        command: WorkshopCommand,
    },
    /// Install and manage official Proton builds.
    Proton {
        #[command(subcommand)]
        command: ProtonCommand,
    },
    /// List and inspect launch profiles (`profiles/<name>.toml`).
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
    /// Manage Steam library collections (categories/groups).
    #[cfg(feature = "collections")]
    Collections {
        #[command(subcommand)]
        command: CollectionsCommand,
    },
    /// List friends, search for a SteamID, or add/remove friends.
    #[cfg(feature = "social")]
    Friends {
        #[command(subcommand)]
        command: Option<FriendsCommand>,
    },
    /// Send and read direct chat messages.
    #[cfg(feature = "social")]
    #[command(after_long_help = RULES_CHAT)]
    Chat {
        #[command(subcommand)]
        command: ChatCommand,
    },
    /// View your Steam inventory for a game.
    #[cfg(feature = "inventory")]
    #[command(after_long_help = RULES_MARKET)]
    Inventory {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Inventory context id (default 2; Steam community items use 6).
        #[arg(short = 'c', long, default_value_t = 2)]
        context: u32,
    },
    /// Show your Steam Wallet balance.
    #[cfg(feature = "inventory")]
    #[command(after_long_help = RULES_MARKET)]
    Wallet,
    /// Steam Community Market: prices, search, and your listings.
    #[cfg(feature = "market")]
    #[command(after_long_help = RULES_MARKET)]
    Market {
        #[command(subcommand)]
        command: MarketCommand,
    },
    /// Inspect launcher configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Clear the stored session.
    #[cfg(feature = "authentication")]
    Logout,
    /// Run the background session daemon serving other commands over a local socket.
    #[cfg(feature = "authentication")]
    Daemon {
        #[command(subcommand)]
        command: Option<DaemonCommand>,
    },
    /// Kill all running gaki-cli processes, including the session daemon.
    #[cfg(feature = "authentication")]
    Kill,
}

#[cfg(feature = "authentication")]
#[derive(Subcommand)]
pub(crate) enum DaemonCommand {
    /// Stop running gaki-cli daemon(s). With a PID, stop only that daemon.
    Stop {
        /// The daemon PID to stop (from `gaki-cli daemon list`). Omit to stop all.
        pid: Option<u32>,
    },
    /// List running gaki-cli daemon(s) with their PID.
    List,
}

#[derive(Subcommand)]
pub(crate) enum ConfigCommand {
    /// Print the settings from `config.toml`, with defaults for unset keys.
    Show,
    /// Check every config file (`config.toml`, `games.toml`, profiles) and
    /// list what's wrong. Fails when there are errors.
    Check,
    /// Print one `config.toml` key as written in the file (nothing when unset).
    Get {
        /// A key such as `default_profile` or `steam_library`.
        key: String,
    },
    /// Set a `config.toml` key, keeping the file's comments. The value is
    /// checked before anything is written.
    Set {
        /// A key such as `default_profile` or `steam_library`.
        key: String,
        /// The value: text, a path, `true`/`false`, or a number.
        value: String,
    },
    /// Remove a `config.toml` key, so it takes its default.
    Unset {
        /// A key such as `default_profile` or `steam_library`.
        key: String,
    },
    /// List detected Proton/Wine runtimes.
    Protons,
    /// View or set the Steam presence the daemon announces for friends/chat.
    Presence {
        /// `online`, or `offline` (invisible: you appear offline but still sync
        /// friends and receive chat). Omit to print the current setting.
        mode: Option<ChatPresenceArg>,
    },
    /// View or set the experimental-features gate.
    ///
    /// When enabled, unlocks experimental commands — currently `login --openid`
    /// (browser identity check) and `login --web-token` (web-surface auth). These
    /// prove identity or enable web-only commands (inventory/wallet/market) but
    /// cannot create the full client session that library/install/launch need.
    /// Omit the value to print it.
    Experimental {
        /// `true` or `false`. Omit to print the current setting.
        enabled: Option<bool>,
    },
    /// View or set whether the daemon resumes interrupted installs when it starts.
    /// `manual` (default) waits for `install resume`; `auto` resumes `interrupted`
    /// backlog entries, never paused or stopped ones.
    InstallResume {
        /// `manual` or `auto`. Omit to print the current setting.
        mode: Option<InstallResumeArg>,
    },
    /// View or set how long `info` results are cached, in seconds (`0` disables
    /// the cache).
    InfoCacheTtl {
        /// Seconds. Omit to print the current setting.
        seconds: Option<u64>,
        /// Revert to the default (6 hours).
        #[arg(short = 'c', long, conflicts_with = "seconds")]
        clear: bool,
    },
    /// View or set the network proxy used for all HTTP(S) communication.
    ///
    /// Applies to the Steam web endpoints, depot downloads, and Proton
    /// release lookups. Takes effect on the next command (and requires restarting
    /// the session daemon). An explicit `HTTP(S)_PROXY`/`ALL_PROXY` environment
    /// variable still overrides this. The Steam CM connection is not proxied.
    ///
    /// Steam's rules forbid using a proxy to disguise your country (Steam
    /// Subscriber Agreement §3.A; see docs/STEAM-RULES.md).
    Proxy {
        /// Proxy URL, e.g. `http://host:8080`, `http://user:pass@host:8080`, or
        /// `socks5://host:1080`. Omit to print the current setting.
        url: Option<String>,
        /// Comma-separated hosts/domains that bypass the proxy (`NO_PROXY`), e.g.
        /// `localhost,127.0.0.1,.internal`.
        #[arg(short = 'n', long, value_name = "LIST")]
        no_proxy: Option<String>,
        /// Clear the configured proxy (revert to a direct connection).
        #[arg(short = 'c', long, conflicts_with_all = ["url", "no_proxy"])]
        clear: bool,
    },
    /// View or set a game's `games.toml` entry (profile, platform, wrappers,
    /// Steam integration, alias).
    Game {
        #[command(flatten)]
        args: GameConfigArgs,
    },
    /// Remove every `games.toml` entry and the old per-game settings.
    ClearGames {
        /// Skip the confirmation prompt.
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProtonCommand {
    /// List installable official Proton builds and what's already installed.
    List {
        /// Only show what's installed on disk.
        #[arg(short = 'i', long)]
        installed: bool,
    },
    /// Install an official Proton build through Steam (names from `proton list`).
    #[cfg(feature = "install")]
    Install {
        /// Runtime name, e.g. `Proton 9.0` or `Proton Experimental`.
        version: String,
    },
    /// Uninstall an official Proton build through Steam.
    #[cfg(feature = "install")]
    Uninstall { version: String },
    /// Set the global default Proton/Wine version (used when a game has none set).
    Default { version: String },
}

#[cfg(feature = "library")]
#[derive(Subcommand)]
pub(crate) enum LibraryCommand {
    /// Write the installed games as `[[game]]` entries (app id, name, the
    /// installed platform and the beta branch). Omit the file to print it.
    Export {
        /// File to write. It is replaced if it exists.
        path: Option<PathBuf>,
        /// Every game in the library, not only the installed ones.
        #[arg(short, long)]
        all: bool,
    },
    /// Install every game in a library file that isn't installed yet, one after
    /// another through the install backlog: `install pause`/`stop` apply, and
    /// `install resume --all` carries on after an interruption. Games already
    /// installed, or not in this account's library, are skipped.
    #[cfg(feature = "install")]
    Import {
        /// Library file, as written by `library export`.
        path: PathBuf,
        /// Steam library folder to install into, as listed by `gaki-cli
        /// libraries`. Defaults to the configured `steam_library_path`.
        #[arg(short = 'l', long)]
        library: Option<String>,
        /// Only report what would be installed and skipped.
        #[arg(short = 'd', long)]
        dry_run: bool,
    },
}

#[cfg(feature = "collections")]
#[derive(Subcommand)]
pub(crate) enum CollectionsCommand {
    /// List all collections and their game counts (offline).
    List,
    /// Show a collection's games (offline). Accepts a name or id.
    Show { name: String },
    /// Create a new (static) collection (offline).
    Create { name: String },
    /// Delete a collection (offline). Built-in favorite/hidden can't be deleted.
    Delete { name: String },
    /// Rename a collection (offline).
    Rename { name: String, new_name: String },
    /// Add one or more app ids to a collection (offline).
    Add {
        name: String,
        #[arg(value_parser = app_ref, required = true)]
        app_ids: Vec<u32>,
    },
    /// Remove one or more app ids from a collection (offline).
    Remove {
        name: String,
        #[arg(value_parser = app_ref, required = true)]
        app_ids: Vec<u32>,
    },
    /// Download collections from your Steam account and merge them in (needs login).
    Pull,
    /// Upload your local collections to your Steam account (needs login).
    /// This changes your real Steam library — confirm or pass `--yes`.
    Push {
        /// Skip the confirmation prompt. Required in `--json` mode.
        #[arg(short, long)]
        yes: bool,
    },
    /// Pull then push — reconcile local and Steam collections (needs login).
    /// This changes your real Steam library — confirm or pass `--yes`.
    Sync {
        /// Skip the confirmation prompt. Required in `--json` mode.
        #[arg(short, long)]
        yes: bool,
    },
}

#[cfg(feature = "social")]
#[derive(Subcommand)]
pub(crate) enum FriendsCommand {
    /// List your friends (the default when no subcommand is given).
    List,
    /// Show a user by id, URL, name.
    Search { query: String },
    /// Send a friend request (same identifiers as search).
    Add { query: String },
    /// Remove a friend, or cancel/decline a pending request (by SteamID64).
    Remove { steamid: u64 },
}

#[cfg(feature = "market")]
#[derive(Subcommand)]
pub(crate) enum MarketCommand {
    /// Item price (community web; no CM equivalent).
    Price {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Exact market hash name (case-sensitive), e.g. "Mann Co. Supply Crate Key".
        name: String,
        /// Steam currency id (1=USD, 2=GBP, 3=EUR, …).
        #[arg(short = 'c', long, default_value_t = 1)]
        currency: u32,
    },
    /// Search the market (community web; no CM equivalent).
    Search {
        /// Free-text query (optional).
        query: Option<String>,
        /// Restrict to one game by app id.
        #[arg(value_parser = app_ref, short = 'a', long)]
        app_id: Option<u32>,
        /// Maximum results to return.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
    },
    /// Show your active market listings and open buy orders.
    Listings,
}

#[cfg(feature = "social")]
#[derive(Subcommand)]
pub(crate) enum ChatCommand {
    /// Send a direct message to a friend (by SteamID64).
    Send {
        steamid: u64,
        /// The message text (all remaining words are joined with spaces).
        #[arg(required = true, trailing_var_arg = true)]
        message: Vec<String>,
    },
    /// Show recent messages exchanged with a friend (by SteamID64).
    History {
        steamid: u64,
        /// How many recent messages to fetch.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
    },
    /// Open an interactive live chat with a friend: type lines to send, incoming
    /// messages stream in. End with Ctrl-D. With `--json`,
    /// emits one JSON event per line and reads message text from stdin lines.
    Open { steamid: u64 },
}

#[cfg(feature = "depot")]
#[derive(Subcommand)]
pub(crate) enum DepotsCommand {
    /// List the files of a depot manifest (default: the depot's current
    /// manifest on the public branch).
    Files {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        depot_id: u32,
        #[command(flatten)]
        manifest: DepotManifestArgs,
        /// Only list paths matching this glob (`*`, `?`; case-insensitive),
        /// e.g. `'*.exe'` or `'bin/*'`.
        #[arg(short = 'f', long)]
        filter: Option<String>,
    },
    /// Download one file of a depot manifest into a directory, under its
    /// own file name. The content is checked against the manifest's SHA-1.
    Download {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        depot_id: u32,
        /// The file's path inside the depot, as `depots files` prints it.
        path: String,
        #[command(flatten)]
        manifest: DepotManifestArgs,
        /// Directory to save into (default: the current directory).
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
}

/// Which manifest of a depot: an id, or a branch's current one.
#[cfg(feature = "depot")]
#[derive(clap::Args)]
pub(crate) struct DepotManifestArgs {
    /// Manifest id (see `gaki-cli manifests`, or SteamDB for older ones).
    #[arg(short = 'm', long, conflicts_with = "branch")]
    pub(crate) manifest: Option<u64>,
    /// Use this branch's current manifest (default: public).
    #[arg(short = 'b', long)]
    pub(crate) branch: Option<String>,
}

#[cfg(feature = "cloud")]
#[derive(Subcommand)]
pub(crate) enum CloudCommand {
    /// Sync a game's Steam Cloud saves with the local save directory. With
    /// neither flag it syncs down then up; `--down`/`--up` restrict the direction.
    Sync {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Only upload local saves to Steam.
        #[arg(short = 'u', long, conflicts_with = "down")]
        up: bool,
        /// Only download saves from Steam.
        #[arg(short = 'd', long, conflicts_with = "up")]
        down: bool,
        /// Local save directory. Defaults to gaki's managed cloud root.
        #[arg(short = 'p', long)]
        path: Option<PathBuf>,
        /// Resolve diverged saves by taking this side (`cloud` or `local`) instead
        /// of reporting them. Omit to only detect conflicts and leave both copies.
        #[arg(short = 'r', long, value_enum)]
        resolve: Option<CloudResolve>,
    },
    /// List a game's Steam Cloud files (name, size, modified time).
    List {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
}

#[cfg(feature = "workshop")]
#[derive(Subcommand)]
pub(crate) enum WorkshopCommand {
    /// Browse/search a game's Workshop to discover items to subscribe to or install.
    Browse {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Filter by search text (free-text match on title/description).
        #[arg(short, long)]
        search: Option<String>,
        /// Sort order.
        #[arg(short = 'o', long, value_enum, default_value_t = WorkshopSort::Trend)]
        sort: WorkshopSort,
        /// Number of results per page (1–100).
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
        /// Pagination cursor; use a previous page's `next_cursor` (`*` = first page).
        #[arg(short = 'c', long, default_value = "*")]
        cursor: String,
        /// Only items carrying this tag (repeatable).
        #[arg(short = 't', long = "tag")]
        tags: Vec<String>,
    },
    /// Show metadata for one or more Workshop items (or collections).
    Info {
        /// One or more Workshop published-file ids.
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// List the Workshop items you're subscribed to for a game.
    List {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Download one or more Workshop items (or collections) and register them.
    Install {
        #[arg(required = true)]
        ids: Vec<u64>,
        /// Install only the given ids; do not expand collections to their members.
        #[arg(short = 'R', long)]
        no_recurse: bool,
    },
    /// Remove one or more installed Workshop items (or collections).
    Uninstall {
        #[arg(required = true)]
        ids: Vec<u64>,
        /// Uninstall only the given ids; do not expand collections to their members.
        #[arg(short = 'R', long)]
        no_recurse: bool,
    },
    /// Subscribe to one or more Workshop items (or collections).
    Subscribe {
        #[arg(required = true)]
        ids: Vec<u64>,
        /// Also download the content after subscribing.
        #[arg(short = 'i', long)]
        install: bool,
        /// Subscribe only to the given ids; do not expand collections to their members.
        #[arg(short = 'R', long)]
        no_recurse: bool,
    },
    /// Unsubscribe from one or more Workshop items (or collections).
    Unsubscribe {
        #[arg(required = true)]
        ids: Vec<u64>,
        /// Unsubscribe only from the given ids; do not expand collections.
        #[arg(short = 'R', long)]
        no_recurse: bool,
    },
    /// Show installed vs subscribed Workshop items for a game.
    Status {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Rate a Workshop item thumbs-up or thumbs-down.
    Rate {
        /// The Workshop published-file id.
        id: u64,
        /// `up` or `down`.
        vote: VoteArg,
    },
    /// Read the comments on a Workshop item.
    Comments {
        /// The Workshop published-file id.
        id: u64,
        /// How many comments to fetch (1–100).
        #[arg(short = 'n', long, default_value_t = 20)]
        count: i32,
        /// Index of the first comment to fetch (for paging).
        #[arg(short = 's', long, default_value_t = 0)]
        start: i32,
    },
    /// Post a comment to a Workshop item.
    Comment {
        /// The Workshop published-file id.
        id: u64,
        /// The comment text.
        text: String,
    },
}

/// `gaki-cli install`: either install an app (positional `app_id`) or manage
/// in-flight installs (`list` / `stop`). The positional and the subcommands
/// coexist via clap's args-conflict-with-subcommands pattern.
#[cfg(feature = "install")]
#[derive(clap::Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub(crate) struct InstallArgs {
    #[command(subcommand)]
    pub(crate) action: Option<InstallAction>,
    /// App id to install.
    #[arg(value_parser = app_ref)]
    pub(crate) app_id: Option<u32>,
    /// Depot platform to install. Defaults to the game's `platform` in
    /// `games.toml`, then `default_platform`, then Windows.
    #[arg(short, long)]
    pub(crate) platform: Option<PlatformArg>,
    /// When installing a DLC, restart the Steam client afterward so the running
    /// client picks up the change. Without this it only warns.
    #[arg(short = 'r', long)]
    pub(crate) restart_steam: bool,
    /// Don't install — just report the estimated download and on-disk size
    /// (from PICS, no files fetched). Pair with `--json` for tooling.
    #[arg(short = 'd', long)]
    pub(crate) dry_run: bool,
    /// Steam library folder (drive/location) to install into. A library root
    /// containing a `steamapps` directory, as listed by `gaki-cli libraries`.
    /// Defaults to the configured `steam_library_path`.
    #[arg(short = 'l', long)]
    pub(crate) library: Option<String>,
}

#[cfg(feature = "install")]
#[derive(clap::Subcommand)]
pub(crate) enum InstallAction {
    /// List running installs and the backlog (paused, aborted, interrupted).
    List,
    /// Stop a running install by app id. It stays in the backlog as `aborted`.
    Stop {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Pause a running install, keeping what's on disk. Resume it with
    /// `install resume`; automatic resume never picks it up.
    Pause {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Resume a backlog install with the options it was started with, reusing
    /// the files already on disk.
    Resume {
        /// App id to resume.
        #[arg(value_parser = app_ref, required_unless_present = "all", conflicts_with = "all")]
        app_id: Option<u32>,
        /// Resume every backlog entry, one after another.
        #[arg(short, long)]
        all: bool,
    },
    /// Remove an install from the backlog.
    Forget {
        #[arg(value_parser = app_ref)]
        app_id: u32,
        /// Also delete its partial files (never a DLC's or a finished game's folder).
        #[arg(short, long)]
        delete_files: bool,
    },
}

/// `gaki-cli downgrade`: install specific depot manifests (an older version) and
/// pin them. `--depot`/`--manifest` are parallel repeatable lists paired by
/// position; `--manifest <depot>:<manifest>` is an alternative combined form.
#[cfg(feature = "versioning")]
#[derive(clap::Args)]
pub(crate) struct DowngradeArgs {
    /// App id to downgrade.
    #[arg(value_parser = app_ref)]
    pub(crate) app_id: u32,
    /// Target depot id (repeatable). Paired by position with a bare `--manifest`.
    #[arg(short = 'd', long = "depot", value_name = "DEPOT_ID")]
    pub(crate) depots: Vec<u32>,
    /// Target manifest id (repeatable). Either a bare id (paired by position with
    /// `--depot`) or the combined `<depot>:<manifest>` form.
    #[arg(short = 'm', long = "manifest", value_name = "MANIFEST_ID")]
    pub(crate) manifests: Vec<String>,
    /// Branch whose build id to record in the appmanifest (default: public).
    #[arg(short = 'b', long)]
    pub(crate) branch: Option<String>,
    /// Password for a protected branch (recorded only; see the downgrade docs).
    #[arg(short = 'w', long)]
    pub(crate) branch_password: Option<String>,
    /// Steam library folder (drive/location) to install into.
    #[arg(short = 'l', long)]
    pub(crate) library: Option<String>,
    /// Verify the install after downloading (integrity pass).
    #[arg(short = 'V', long)]
    pub(crate) verify: bool,
    /// Don't pin after downgrading (gaki's update commands may re-upgrade it).
    #[arg(short = 'N', long)]
    pub(crate) no_pin: bool,
}

/// `gaki-cli login` flags.
#[cfg(feature = "authentication")]
#[derive(clap::Args)]
pub(crate) struct LoginArgs {
    /// Steam account name (prompted if omitted).
    #[arg(short, long)]
    pub(crate) username: Option<String>,
    /// Password (prompted if omitted). Visible to other local users in the
    /// process list; prefer `--password-stdin` in scripts.
    #[arg(short, long)]
    pub(crate) password: Option<String>,
    /// Read the password from the first line of stdin.
    #[arg(long, conflicts_with = "password")]
    pub(crate) password_stdin: bool,
    /// Steam Guard code, if known.
    #[arg(short, long)]
    pub(crate) guard: Option<String>,
    /// Log in via Steam Mobile QR code.
    #[arg(short = 'q', long, conflicts_with_all = ["username", "password", "password_stdin", "guard", "code"])]
    pub(crate) qr: bool,
    /// With `--qr`: give up after this many seconds without approval (default 600).
    /// With `--json`, a `cancel` line or EOF on stdin also ends the wait.
    #[arg(short = 't', long, value_name = "SECS", requires = "qr", value_parser = clap::value_parser!(u64).range(1..))]
    pub(crate) timeout: Option<u64>,
    /// Type the Guard code instead of app approval.
    #[arg(short = 'c', long, visible_alias = "pin", conflicts_with = "guard")]
    pub(crate) code: bool,
    /// Experimental: browser OpenID identity check (no session token).
    #[arg(short = 'o', long, hide = true, conflicts_with_all = ["username", "password", "password_stdin", "guard", "qr", "code"])]
    pub(crate) openid: bool,
    /// Experimental: store a web token from steamcommunity.com/chat/clientjstoken
    /// (the value, or stdin when omitted) for web-only commands; expires in ~24h.
    #[arg(short = 'w', long, hide = true, num_args = 0..=1, conflicts_with_all = ["username", "password", "password_stdin", "guard", "qr", "code", "openid"])]
    pub(crate) web_token: Option<Option<String>>,
    /// Report session health without logging in.
    #[arg(short = 'H', long, conflicts_with_all = ["username", "password", "password_stdin", "guard", "qr", "code", "openid", "web_token", "reconnect"])]
    pub(crate) health: bool,
    /// Re-establish the daemon session (daemon required).
    #[arg(short = 'r', long, conflicts_with_all = ["username", "password", "password_stdin", "guard", "qr", "code", "openid", "web_token", "health"])]
    pub(crate) reconnect: bool,
}

/// `gaki-cli reviews` flags.
#[cfg(feature = "store")]
#[derive(clap::Args)]
pub(crate) struct ReviewsArgs {
    #[arg(value_parser = app_ref)]
    pub(crate) app_id: u32,
    /// `recent`, `updated` or `all` (score-weighted).
    #[arg(short = 'f', long, default_value = "recent")]
    pub(crate) filter: String,
    /// `all`, `positive` or `negative`.
    #[arg(short = 't', long = "type", default_value = "all")]
    pub(crate) review_type: String,
    /// `all`, `steam` or `non_steam_purchase`.
    #[arg(short = 'p', long, default_value = "all")]
    pub(crate) purchase: String,
    /// Maximum reviews per page (up to 100).
    #[arg(short = 'n', long, default_value_t = 10)]
    pub(crate) count: u32,
    /// Cursor from a previous page's JSON.
    #[arg(short = 'u', long)]
    pub(crate) cursor: Option<String>,
}

/// `list --sort` choices.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum ListSort {
    /// By name.
    Name,
    /// Most total playtime first.
    Playtime,
    /// Most recently played first.
    Recent,
}

#[cfg(feature = "library")]
impl From<ListSort> for libgaki::ops::library::SortBy {
    fn from(value: ListSort) -> Self {
        match value {
            ListSort::Name => Self::Name,
            ListSort::Playtime => Self::Playtime,
            ListSort::Recent => Self::LastPlayed,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum PlatformArg {
    Windows,
    Linux,
}

/// `config install-resume` choices.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum InstallResumeArg {
    /// Wait for `install resume`.
    Manual,
    /// Daemon resumes interrupted installs.
    Auto,
}

impl From<InstallResumeArg> for libgaki::library::backlog::InstallResume {
    fn from(value: InstallResumeArg) -> Self {
        match value {
            InstallResumeArg::Manual => Self::Manual,
            InstallResumeArg::Auto => Self::Auto,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum ChatPresenceArg {
    Online,
    Offline,
}

impl From<ChatPresenceArg> for libgaki::core::config::ChatPresence {
    fn from(value: ChatPresenceArg) -> Self {
        match value {
            ChatPresenceArg::Online => libgaki::core::config::ChatPresence::Online,
            ChatPresenceArg::Offline => libgaki::core::config::ChatPresence::Offline,
        }
    }
}

#[cfg(feature = "cloud")]
/// Which side wins when `cloud sync` finds a diverged save.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum CloudResolve {
    /// Overwrite the local copy with the cloud copy.
    Cloud,
    /// Overwrite the cloud copy with the local copy.
    Local,
}

#[cfg(feature = "cloud")]
impl From<CloudResolve> for libgaki::library::cloud_sync::ConflictPolicy {
    fn from(value: CloudResolve) -> Self {
        match value {
            CloudResolve::Cloud => libgaki::library::cloud_sync::ConflictPolicy::TakeCloud,
            CloudResolve::Local => libgaki::library::cloud_sync::ConflictPolicy::TakeLocal,
        }
    }
}

#[cfg(feature = "workshop")]
/// Sort order for `workshop browse`, mapped to an `EPublishedFileQueryType`.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum WorkshopSort {
    /// Trending now (default).
    Trend,
    /// Highest rated overall.
    Popular,
    /// Most recently published.
    Recent,
    /// Most recently updated.
    Updated,
    /// Most subscribed.
    Subscriptions,
    /// Best text-search relevance (use with `--search`).
    Text,
}

#[cfg(feature = "workshop")]
impl From<WorkshopSort> for libgaki::ops::workshop::Sort {
    fn from(value: WorkshopSort) -> Self {
        use libgaki::ops::workshop::Sort;
        match value {
            WorkshopSort::Trend => Sort::Trend,
            WorkshopSort::Popular => Sort::Popular,
            WorkshopSort::Recent => Sort::Recent,
            WorkshopSort::Updated => Sort::Updated,
            WorkshopSort::Subscriptions => Sort::Subscriptions,
            WorkshopSort::Text => Sort::Text,
        }
    }
}

#[cfg(feature = "workshop")]
/// Thumbs-up/down for `workshop rate`.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum VoteArg {
    Up,
    Down,
}

impl From<PlatformArg> for DepotPlatform {
    fn from(value: PlatformArg) -> Self {
        match value {
            PlatformArg::Windows => DepotPlatform::Windows,
            PlatformArg::Linux => DepotPlatform::Linux,
        }
    }
}

/// `profiles list|show`.
#[derive(Subcommand)]
pub(crate) enum ProfilesCommand {
    /// List every profile: runner, Proton build, prefix, and the games using it.
    List,
    /// Print a profile's file, the games using it, and any problems.
    Show {
        /// The profile's name (its file name without `.toml`).
        name: String,
    },
}

#[cfg(feature = "store")]
/// `wishlist add|remove`.
#[derive(Subcommand)]
pub(crate) enum WishlistCommand {
    /// Add an app to your wishlist.
    Add {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
    /// Remove an app from your wishlist.
    Remove {
        #[arg(value_parser = app_ref)]
        app_id: u32,
    },
}

/// `deals --scope` choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum DealsScopeArg {
    /// Discounted items among regional top sellers.
    DiscountedTopSellers,
    /// Regional top sellers, discounted or not.
    TopSellers,
    /// Anything discounted at least `--min-discount`.
    Specials,
}

/// Grouped `config game` flags.
#[derive(clap::Args)]
pub(crate) struct GameConfigArgs {
    #[arg(value_parser = app_ref)]
    pub(crate) app_id: u32,
    /// Run the game with this profile (`gaki-cli profiles list`).
    #[arg(short = 'p', long)]
    pub(crate) profile: Option<String>,
    /// Remove the game's profile (it uses `default_profile`).
    #[arg(short = 'P', long, conflicts_with = "profile")]
    pub(crate) no_profile: bool,
    /// Run this build (`windows` runs through Proton on Linux).
    #[arg(short = 't', long)]
    pub(crate) platform: Option<PlatformArg>,
    /// Remove the game's platform (back to the profile or `default_platform`).
    #[arg(short = 'T', long, conflicts_with = "platform")]
    pub(crate) no_platform: bool,
    /// Wrap the launch in this command, inside the profile's wrappers; repeat
    /// for several, outermost first. Replaces the game's wrappers. A script
    /// works: it gets the command as `"$@"` and the `GAKI_*` variables.
    #[arg(short = 'w', long = "wrapper", value_name = "COMMAND")]
    pub(crate) wrappers: Vec<String>,
    /// Remove the game's wrappers (the profile's still apply).
    #[arg(short = 'W', long, conflicts_with = "wrappers")]
    pub(crate) no_wrappers: bool,
    /// Steam integration: `auto`, `host` or `none`.
    #[arg(long, value_name = "MODE")]
    pub(crate) steam: Option<SteamArg>,
    /// Remove the game's Steam integration setting (the profile's applies).
    #[arg(long, conflicts_with = "steam")]
    pub(crate) no_steam: bool,
    /// An alias for the game, usable wherever an app id is.
    #[arg(short = 'n', long, value_name = "ALIAS")]
    pub(crate) name: Option<String>,
    /// Remove the game's alias.
    #[arg(short = 'N', long, conflicts_with = "name")]
    pub(crate) no_name: bool,
    /// Remove the game's entry and its old per-game settings.
    #[arg(long, conflicts_with_all = ["profile", "no_profile", "platform", "no_platform",
        "wrappers", "no_wrappers", "steam", "no_steam", "name", "no_name"])]
    pub(crate) clear: bool,
}

/// `--steam` modes for `config game`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum SteamArg {
    Auto,
    Host,
    None,
}

impl From<SteamArg> for libgaki::config::SteamIntegration {
    fn from(arg: SteamArg) -> Self {
        match arg {
            SteamArg::Auto => Self::Auto,
            SteamArg::Host => Self::Host,
            SteamArg::None => Self::None,
        }
    }
}
