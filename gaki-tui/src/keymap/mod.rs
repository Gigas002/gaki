//! Key bindings, from `$XDG_CONFIG_HOME/gaki/tui/keybinds.toml`.
//!
//! One table per context maps action names to a key or a list of keys:
//!
//! ```toml
//! [library]
//! install = "i"
//! sort = ["s", "ctrl+s"]
//! hide = []            # no key; still in the Actions menu
//! ```
//!
//! `[library]` is the library with no overlay open; `[overlay]` holds the
//! keys every overlay shares (moving, tabs, closing); each overlay has a
//! section of its own (`[installs]`, `[settings]`, …). An action left out
//! keeps its default, so an empty or missing file changes nothing. The file is read once at startup, and gaki-tui refuses to start
//! on an unknown section or action, a key it can't parse, a reserved key, or
//! two actions on one key in the same context, naming the line.
//!
//! Some keys can't be rebound, so a bad file can't lock anyone out: `Esc`,
//! `?` (help), the arrow keys and Ctrl+C (quit) always do what they do.

mod key;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ratatui::crossterm::event::KeyCode;

pub use key::Key;

/// One context's actions: a section of `keybinds.toml`.
pub trait Actions: Copy + Eq + Ord + std::hash::Hash + fmt::Debug + Send + Sync + 'static {
    /// The section name, `[library]`.
    const SECTION: &'static str;
    /// Whether the context takes typed text, so plain characters can't be
    /// bound.
    const TEXT: bool = false;
    /// Whether the `[overlay]` keys work here too, so they mustn't clash.
    const SHARES_OVERLAY: bool = false;

    /// Every action, in the order `keybinds.toml` lists them.
    fn all() -> &'static [Self];
    /// The name `keybinds.toml` uses.
    fn name(self) -> &'static str;
    /// The keys it has when the file doesn't say.
    fn default_keys(self) -> &'static [&'static str];
    /// The tab it belongs to, when the context has tabs; actions in
    /// different tabs may share a key.
    fn group(self) -> Option<&'static str>;
    /// A fixed key shown after the bound ones (`↓` for the arrow).
    fn alias(self) -> Option<&'static str>;
    /// Keys the context keeps for itself (a prompt's `Enter`).
    fn taken() -> &'static [&'static str] {
        &[]
    }
    fn bindings(map: &Keymap) -> &Bindings<Self>;
    fn bindings_mut(map: &mut Keymap) -> &mut Bindings<Self>;
}

macro_rules! actions {
    (
        $(#[$meta:meta])*
        $ty:ident, $section:literal, $field:ident $(, $flag:ident)*
            $(; taken [$($taken:literal),*])? {
            $($variant:ident => $name:literal [$($key:literal),*]
                $(in $group:literal)? $(alias $alias:literal)?,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $ty {
            $($variant),*
        }

        impl Actions for $ty {
            const SECTION: &'static str = $section;
            $(const $flag: bool = true;)*

            fn all() -> &'static [Self] {
                &[$($ty::$variant),*]
            }

            fn name(self) -> &'static str {
                match self {
                    $($ty::$variant => $name),*
                }
            }

            fn default_keys(self) -> &'static [&'static str] {
                match self {
                    $($ty::$variant => &[$($key),*]),*
                }
            }

            fn group(self) -> Option<&'static str> {
                match self {
                    $($ty::$variant => None::<&'static str> $(.or(Some($group)))?),*
                }
            }

            fn alias(self) -> Option<&'static str> {
                match self {
                    $($ty::$variant => None::<&'static str> $(.or(Some($alias)))?),*
                }
            }

            $(fn taken() -> &'static [&'static str] {
                &[$($taken),*]
            })?

            fn bindings(map: &Keymap) -> &Bindings<Self> {
                &map.$field
            }

            fn bindings_mut(map: &mut Keymap) -> &mut Bindings<Self> {
                &mut map.$field
            }
        }
    };
}

actions! {
    /// What a key in the library (no overlay open) does.
    LibraryAction, "library", library {
        Quit => "quit" ["q"],
        Down => "down" ["j"] alias "\u{2193}",
        Up => "up" ["k"] alias "\u{2191}",
        Top => "top" ["g"],
        Bottom => "bottom" ["G"],
        PageDown => "page_down" ["PageDown"],
        PageUp => "page_up" ["PageUp"],
        Filter => "filter" ["/"],
        NextTab => "next_tab" ["Tab"],
        PreviousTab => "previous_tab" ["BackTab"],
        TabAll => "tab_all" ["1"],
        TabInstalled => "tab_installed" ["2"],
        TabUpdates => "tab_updates" ["3"],
        TabFavourites => "tab_favourites" ["4"],
        TabFriends => "tab_friends" ["5"],
        Friends => "friends" ["F"],
        Sort => "sort" ["s"],
        Description => "description" ["i"],
        ActionsMenu => "actions_menu" ["Enter"],
        Install => "install" ["d"],
        PauseResume => "pause_resume" ["Space"],
        CancelInstall => "cancel_install" ["c"],
        Uninstall => "uninstall" ["x"],
        MoveInstall => "move_install" ["M"],
        Relink => "relink" ["K"],
        Import => "import" ["N"],
        Verify => "verify" ["v"],
        Update => "update" ["U"],
        Versions => "versions" ["V"],
        Branches => "branches" ["b"],
        Collections => "collections" ["O"],
        Achievements => "achievements" ["a"],
        Inventory => "inventory" ["I"],
        MarketListings => "market_listings" ["m"],
        MarketSearch => "market_search" ["S"],
        Workshop => "workshop" ["W"],
        Dlc => "dlc" ["D"],
        Depots => "depots" ["o"],
        Proton => "proton" ["P"],
        Running => "running" ["R"],
        Store => "store" ["Y"],
        SteamStore => "steam_store" ["J"],
        Installs => "installs" ["X"],
        GameInfo => "game_info" ["n"],
        Cloud => "cloud" ["C"],
        LaunchOptions => "launch_options" ["L"],
        Favourite => "favourite" ["f"],
        Hide => "hide" ["H"],
        SteamPage => "steam_page" ["E"],
        ProtonDb => "protondb" ["B"],
        ProtonIssues => "proton_issues" ["T"],
        SteamDb => "steamdb" ["Z"],
        Reload => "reload" ["r"],
        Account => "account" ["A"],
        Settings => "settings" ["p"],
        Wallet => "wallet" ["w"],
        SignIn => "sign_in" ["l"],
    }
}

actions! {
    /// Keys every overlay shares: moving, scrolling, tabs, closing.
    OverlayAction, "overlay", overlay {
        Close => "close" ["q"],
        Down => "down" ["j"] alias "\u{2193}",
        Up => "up" ["k"] alias "\u{2191}",
        PageDown => "page_down" ["PageDown"],
        PageUp => "page_up" ["PageUp"],
        NextTab => "next_tab" ["Tab"],
        PreviousTab => "previous_tab" ["BackTab"],
    }
}

actions! {
    /// The Installs overlay (`X`): the queue and the Duplicates tab.
    InstallsAction, "installs", installs, SHARES_OVERLAY {
        Resume => "resume" ["r", "Enter"] in "queue",
        ResumeAll => "resume_all" ["R"] in "queue",
        Pause => "pause" ["p"] in "queue",
        Stop => "stop" ["s"] in "queue",
        Forget => "forget" ["f"] in "queue",
        ForgetAndDelete => "forget_and_delete" ["F"] in "queue",
        Rescan => "rescan" ["r"] in "duplicates",
    }
}

actions! {
    /// The Settings overlay (`p`), by tab.
    SettingsAction, "settings", settings, SHARES_OVERLAY {
        Edit => "edit" ["Enter"] in "settings",
        Unset => "unset" ["x"] in "settings",
        MakeDefault => "make_default" ["d"] in "profiles",
        CheckAgain => "check_again" ["r"] in "check",
        StopDaemon => "stop_daemon" ["s"] in "daemons",
        StopAllDaemons => "stop_all_daemons" ["S"] in "daemons",
        RefreshDaemons => "refresh_daemons" ["r"] in "daemons",
    }
}

actions! {
    /// The Workshop overlay's status list (`W`).
    WorkshopAction, "workshop", workshop, SHARES_OVERLAY {
        Browse => "browse" ["b"],
        Install => "install" ["i"],
        Uninstall => "uninstall" ["u"],
        UpdateAll => "update_all" ["U"],
        Reload => "reload" ["r"],
    }
}

actions! {
    /// The Workshop browse pane. It takes the search text, so only
    /// non-character keys can be bound.
    WorkshopBrowseAction, "workshop_browse", workshop_browse, TEXT;
        taken ["Enter", "Backspace"] {
        Subscribe => "subscribe" ["Tab"],
        RateUp => "rate_up" ["F1"],
        RateDown => "rate_down" ["F2"],
        Comments => "comments" ["F3"],
        Install => "install" ["F4"],
    }
}

actions! {
    /// The Workshop comments sub-pane.
    WorkshopCommentsAction, "workshop_comments", workshop_comments, SHARES_OVERLAY {
        Write => "write" ["c"],
    }
}

actions! {
    /// The per-game Store overlay (`Y`).
    StoreAction, "store", store, SHARES_OVERLAY {
        NextSection => "next_section" ["l"],
        PreviousSection => "previous_section" ["h"],
        Regions => "regions" ["r"],
    }
}

actions! {
    /// The Steam Store overlay (`J`).
    SteamStoreAction, "steam_store", steam_store, SHARES_OVERLAY {
        NextSection => "next_section" ["l"],
        PreviousSection => "previous_section" ["h"],
        Details => "details" ["Enter"],
        Search => "search" ["/"],
        Wishlist => "wishlist" ["w"],
        StorePage => "store_page" ["E"],
        NextDeals => "next_deals" ["s"],
        Reload => "reload" ["r"],
    }
}

actions! {
    /// The Depots overlay (`o`).
    DepotsAction, "depots", depots, SHARES_OVERLAY {
        Files => "files" ["Enter"],
        SteamDb => "steamdb" ["Z"],
    }
}

actions! {
    /// The depot file view.
    DepotFilesAction, "depot_files", depot_files, SHARES_OVERLAY {
        Filter => "filter" ["/"],
        Branch => "branch" ["b"],
        Download => "download" ["d"],
    }
}

actions! {
    /// The Game info overlay (`n`), by tab.
    GameInfoAction, "game_info", game_info, SHARES_OVERLAY {
        CheckAgain => "check_again" ["r"] in "checks",
        SaveArtwork => "save_artwork" ["s"] in "artwork",
        ArtworkFolder => "artwork_folder" ["o"] in "artwork",
    }
}

actions! {
    /// The Cloud saves overlay (`C`).
    CloudAction, "cloud", cloud, SHARES_OVERLAY {
        Sync => "sync" ["s"],
        Download => "download" ["d"],
        Upload => "upload" ["u"],
        KeepCloud => "keep_cloud" ["c"],
        KeepLocal => "keep_local" ["l"],
    }
}

actions! {
    /// The Collections overlay (`O`).
    CollectionsAction, "collections", collections, SHARES_OVERLAY {
        Add => "add" ["a"],
        Remove => "remove" ["r"],
        New => "new" ["n"],
        Rename => "rename" ["e"],
        Delete => "delete" ["x"],
        Pull => "pull" ["P"],
        Push => "push" ["U"],
        Sync => "sync" ["S"],
    }
}

actions! {
    /// The Versions overlay (`V`).
    VersionsAction, "versions", versions, SHARES_OVERLAY {
        Pin => "pin" ["p"],
        Unpin => "unpin" ["u"],
        Downgrade => "downgrade" ["d"],
        SteamDb => "steamdb" ["Z"],
    }
}

actions! {
    /// The launch target picker (Play as).
    LaunchTargetsAction, "launch_targets", launch_targets, SHARES_OVERLAY {
        Launch => "launch" ["Enter"],
    }
}

actions! {
    /// The per-game settings overlay.
    GameSettingsAction, "game_settings", game_settings, SHARES_OVERLAY {
        Edit => "edit" ["Enter"],
    }
}

actions! {
    /// The install location picker.
    InstallPickerAction, "install_picker", install_picker, SHARES_OVERLAY {
        Install => "install" ["Enter"],
    }
}

actions! {
    /// The DLC overlay (`D`).
    DlcAction, "dlc", dlc, SHARES_OVERLAY {
        Enable => "enable" ["e"],
        Disable => "disable" ["x"],
    }
}

actions! {
    /// The beta branches overlay (`b`).
    BranchesAction, "branches", branches, SHARES_OVERLAY {
        Switch => "switch" ["Enter", "s"],
    }
}

actions! {
    /// The running games overlay (`R`).
    RunningAction, "running", running, SHARES_OVERLAY {
        Stop => "stop" ["s", "x"],
    }
}

actions! {
    /// The Proton overlay (`P`).
    ProtonAction, "proton", proton, SHARES_OVERLAY {
        MakeDefault => "make_default" ["d"],
        Install => "install" ["i"],
        Uninstall => "uninstall" ["u"],
    }
}

actions! {
    /// The account overlay (`A`); any other key closes it.
    AccountAction, "account", account {
        LogOut => "log_out" ["o"],
    }
}

actions! {
    /// The market search overlay (`S`). It takes the query, so only keys
    /// that don't type can be bound.
    MarketSearchAction, "market_search", market_search, TEXT;
        taken ["Enter", "Backspace"] {
        Price => "price" ["Tab"],
    }
}

actions! {
    /// The Friends tab. Moving and leaving (`close`) come from
    /// `[overlay]`; `Tab` cycles the library's tabs.
    FriendsAction, "friends", friends, SHARES_OVERLAY {
        Chat => "chat" ["Enter", "c"],
        ChatWindow => "chat_window" ["t"],
        Add => "add" ["a"],
        Profile => "profile" ["v"],
        Lookup => "lookup" ["/"],
        Remove => "remove" ["x"],
        Leave => "leave" ["F"],
    }
}

actions! {
    /// A user's profile (`v` in the Friends tab).
    UserProfileAction, "user_profile", user_profile, SHARES_OVERLAY {
        Lookup => "lookup" ["/"],
        ProfilePage => "profile_page" ["E"],
    }
}

actions! {
    /// The Actions menu (`Enter`). It filters as you type, so only keys
    /// that don't type can be bound; the arrows always move.
    ActionsMenuAction, "actions_menu", actions_menu, TEXT; taken ["Backspace"] {
        Run => "run" ["Enter"],
        Next => "next" [],
        Previous => "previous" [],
    }
}

/// Keys that keep their meaning whatever the file says.
pub const RESERVED: &[Key] = &[
    Key::plain(KeyCode::Esc),
    Key::plain(KeyCode::Char('?')),
    Key::plain(KeyCode::Up),
    Key::plain(KeyCode::Down),
    Key::plain(KeyCode::Left),
    Key::plain(KeyCode::Right),
    Key::ctrl('c'),
];

/// Why `keybinds.toml` was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapError {
    pub path: Option<PathBuf>,
    /// 1-based, when the problem is on one line.
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for KeymapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}", path.display())?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            f.write_str(": ")?;
        } else if let Some(line) = self.line {
            write!(f, "line {line}: ")?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for KeymapError {}

/// One context's bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings<A: Actions> {
    keys: BTreeMap<A, Vec<Key>>,
    /// The line each action the file set is on, for errors.
    lines: BTreeMap<A, usize>,
}

impl<A: Actions> Default for Bindings<A> {
    fn default() -> Self {
        let keys = A::all()
            .iter()
            .map(|&a| {
                let keys = a.default_keys().iter().map(|k| {
                    Key::parse(k).unwrap_or_else(|e| panic!("default key for {a:?}: {e}"))
                });
                (a, keys.collect())
            })
            .collect();
        Self {
            keys,
            lines: BTreeMap::new(),
        }
    }
}

impl<A: Actions> Bindings<A> {
    fn keys(&self, action: A) -> &[Key] {
        self.keys
            .get(&action)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The action `key` runs in tab `group` (an action without a tab runs
    /// in every one).
    fn find(&self, key: Key, group: Option<&str>) -> Option<A> {
        A::all().iter().copied().find(|&a| {
            (a.group().is_none() || group.is_none() || a.group() == group)
                && self.keys(a).contains(&key)
        })
    }
}

/// Whether two actions can be reached at the same moment.
fn same_tab(a: Option<&str>, b: Option<&str>) -> bool {
    a.is_none() || b.is_none() || a == b
}

/// The bindings in effect.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keymap {
    library: Bindings<LibraryAction>,
    overlay: Bindings<OverlayAction>,
    installs: Bindings<InstallsAction>,
    settings: Bindings<SettingsAction>,
    workshop: Bindings<WorkshopAction>,
    workshop_browse: Bindings<WorkshopBrowseAction>,
    workshop_comments: Bindings<WorkshopCommentsAction>,
    store: Bindings<StoreAction>,
    steam_store: Bindings<SteamStoreAction>,
    depots: Bindings<DepotsAction>,
    depot_files: Bindings<DepotFilesAction>,
    game_info: Bindings<GameInfoAction>,
    cloud: Bindings<CloudAction>,
    collections: Bindings<CollectionsAction>,
    versions: Bindings<VersionsAction>,
    launch_targets: Bindings<LaunchTargetsAction>,
    game_settings: Bindings<GameSettingsAction>,
    install_picker: Bindings<InstallPickerAction>,
    dlc: Bindings<DlcAction>,
    branches: Bindings<BranchesAction>,
    running: Bindings<RunningAction>,
    proton: Bindings<ProtonAction>,
    account: Bindings<AccountAction>,
    market_search: Bindings<MarketSearchAction>,
    friends: Bindings<FriendsAction>,
    user_profile: Bindings<UserProfileAction>,
    actions_menu: Bindings<ActionsMenuAction>,
}

type Section = BTreeMap<String, toml::Spanned<toml::Value>>;

/// What each section does: read its lines into the map, and check it.
struct SectionKind {
    name: &'static str,
    apply: fn(&mut Keymap, &Section, &str) -> Result<(), KeymapError>,
    check: fn(&Keymap) -> Result<(), KeymapError>,
}

fn kind<A: Actions>() -> SectionKind {
    SectionKind {
        name: A::SECTION,
        apply: apply::<A>,
        check: check::<A>,
    }
}

/// Every section, `[library]` first.
fn sections() -> [SectionKind; 27] {
    [
        kind::<LibraryAction>(),
        kind::<OverlayAction>(),
        kind::<InstallsAction>(),
        kind::<SettingsAction>(),
        kind::<WorkshopAction>(),
        kind::<WorkshopBrowseAction>(),
        kind::<WorkshopCommentsAction>(),
        kind::<StoreAction>(),
        kind::<SteamStoreAction>(),
        kind::<DepotsAction>(),
        kind::<DepotFilesAction>(),
        kind::<GameInfoAction>(),
        kind::<CloudAction>(),
        kind::<CollectionsAction>(),
        kind::<VersionsAction>(),
        kind::<LaunchTargetsAction>(),
        kind::<GameSettingsAction>(),
        kind::<InstallPickerAction>(),
        kind::<DlcAction>(),
        kind::<BranchesAction>(),
        kind::<RunningAction>(),
        kind::<ProtonAction>(),
        kind::<AccountAction>(),
        kind::<MarketSearchAction>(),
        kind::<FriendsAction>(),
        kind::<UserProfileAction>(),
        kind::<ActionsMenuAction>(),
    ]
}

/// The section names, for the docs and errors.
pub fn section_names() -> Vec<&'static str> {
    sections().iter().map(|s| s.name).collect()
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

fn error(line: Option<usize>, message: impl Into<String>) -> KeymapError {
    KeymapError {
        path: None,
        line,
        message: message.into(),
    }
}

/// A value's keys: one string or a list of them.
fn keys_of(value: &toml::Value) -> Option<Vec<&str>> {
    match value {
        toml::Value::String(s) => Some(vec![s.as_str()]),
        toml::Value::Array(items) => items.iter().map(toml::Value::as_str).collect(),
        _ => None,
    }
}

/// Whether `key` would type into a text context.
fn types_text(key: Key) -> bool {
    !key.ctrl && !key.alt && matches!(key.code, KeyCode::Char(_))
}

fn apply<A: Actions>(map: &mut Keymap, section: &Section, text: &str) -> Result<(), KeymapError> {
    let bindings = A::bindings_mut(map);
    for (action_name, value) in section {
        let line_no = line_of(text, value.span().start);
        let line = Some(line_no);
        let Some(&action) = A::all().iter().find(|a| a.name() == action_name.as_str()) else {
            return Err(error(
                line,
                format!("unknown action `{action_name}` in [{}]", A::SECTION),
            ));
        };
        let Some(texts) = keys_of(value.get_ref()) else {
            return Err(error(
                line,
                format!("`{action_name}`: expected a key or a list of keys"),
            ));
        };
        let mut parsed: Vec<Key> = Vec::new();
        for t in texts {
            let key = Key::parse(t).map_err(|m| error(line, m))?;
            if RESERVED.contains(&key) {
                return Err(error(
                    line,
                    format!("`{key}` is reserved and can't be bound (`{action_name}`)"),
                ));
            }
            if A::taken()
                .iter()
                .any(|t| Key::parse(t).is_ok_and(|t| t == key))
            {
                return Err(error(
                    line,
                    format!(
                        "`{key}` is fixed in [{}] and can't be bound (`{action_name}`)",
                        A::SECTION
                    ),
                ));
            }
            if A::TEXT && types_text(key) {
                return Err(error(
                    line,
                    format!(
                        "`{key}` types into the text in [{}]; use a key that doesn't, like F5 or ctrl+{key}",
                        A::SECTION
                    ),
                ));
            }
            if !parsed.contains(&key) {
                parsed.push(key);
            }
        }
        bindings.keys.insert(action, parsed);
        bindings.lines.insert(action, line_no);
    }
    Ok(())
}

/// No two actions reachable at once share a key, within the section and,
/// for an overlay, against `[overlay]`.
fn check<A: Actions>(map: &Keymap) -> Result<(), KeymapError> {
    let bindings = A::bindings(map);
    let clash = |key: Key, a: String, b: String, line: Option<usize>| {
        error(
            line,
            format!("`{key}` is bound to both {a} and {b}; give one of them another key"),
        )
    };
    let all = A::all();
    for (i, &a) in all.iter().enumerate() {
        for &b in &all[i + 1..] {
            if !same_tab(a.group(), b.group()) {
                continue;
            }
            if let Some(&key) = bindings
                .keys(a)
                .iter()
                .find(|k| bindings.keys(b).contains(k))
            {
                let line = bindings
                    .lines
                    .get(&b)
                    .or_else(|| bindings.lines.get(&a))
                    .copied();
                return Err(clash(
                    key,
                    format!("`{}`", a.name()),
                    format!("`{}` in [{}]", b.name(), A::SECTION),
                    line,
                ));
            }
        }
    }
    if A::SHARES_OVERLAY {
        let overlay = &map.overlay;
        for &a in all {
            for &o in OverlayAction::all() {
                if let Some(&key) = bindings
                    .keys(a)
                    .iter()
                    .find(|k| overlay.keys(o).contains(k))
                {
                    let line = bindings
                        .lines
                        .get(&a)
                        .or_else(|| overlay.lines.get(&o))
                        .copied();
                    return Err(clash(
                        key,
                        format!("`{}` in [overlay]", o.name()),
                        format!("`{}` in [{}]", a.name(), A::SECTION),
                        line,
                    ));
                }
            }
        }
    }
    Ok(())
}

impl Keymap {
    /// Every action on its default keys.
    pub fn defaults() -> Self {
        Self::default()
    }

    /// Read `path`; a missing file is the defaults.
    pub fn load(path: &Path) -> Result<Self, KeymapError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::defaults()),
            Err(e) => {
                return Err(KeymapError {
                    path: Some(path.to_path_buf()),
                    line: None,
                    message: format!("couldn't read it: {e}"),
                });
            }
        };
        Self::parse(&text).map_err(|e| KeymapError {
            path: Some(path.to_path_buf()),
            ..e
        })
    }

    /// Parse a `keybinds.toml`.
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        let file: BTreeMap<String, toml::Spanned<Section>> = toml::from_str(text).map_err(|e| {
            error(
                e.span().map(|s| line_of(text, s.start)),
                e.message().to_string(),
            )
        })?;
        let kinds = sections();
        let mut map = Self::defaults();
        for (name, section) in &file {
            let Some(kind) = kinds.iter().find(|k| k.name == name.as_str()) else {
                return Err(error(
                    Some(line_of(text, section.span().start)),
                    format!(
                        "unknown section `[{name}]`; the sections are {}",
                        section_names().join(", ")
                    ),
                ));
            };
            (kind.apply)(&mut map, section.get_ref(), text)?;
        }
        for kind in &kinds {
            (kind.check)(&map)?;
        }
        Ok(map)
    }

    /// What `key` does in context `A`, in tab `group` when it has tabs
    /// (`None` matches every tab).
    pub fn find<A: Actions>(&self, key: Key, group: Option<&str>) -> Option<A> {
        A::bindings(self).find(key, group)
    }

    /// What `key` does in the library.
    pub fn library_action(&self, key: Key) -> Option<LibraryAction> {
        self.find(key, None)
    }

    /// What `key` does in every overlay.
    pub fn overlay_action(&self, key: Key) -> Option<OverlayAction> {
        self.find(key, None)
    }

    /// The keys bound to `action`.
    pub fn keys<A: Actions>(&self, action: A) -> &[Key] {
        A::bindings(self).keys(action)
    }

    /// `action`'s keys for the help and the hints, `j / ↓` style; empty
    /// when it has none.
    pub fn label<A: Actions>(&self, action: A) -> String {
        let mut labels: Vec<String> = self.keys(action).iter().map(Key::to_string).collect();
        if let Some(alias) = action.alias() {
            labels.push(alias.to_string());
        }
        labels.join(" / ")
    }

    /// `action`'s first key, for the one-key hints; empty when it has none.
    pub fn first<A: Actions>(&self, action: A) -> String {
        self.keys(action)
            .first()
            .map(Key::to_string)
            .unwrap_or_default()
    }
}

/// `[key] what  [key] what …` from `(action's first key, what)` pairs,
/// leaving out actions with no key. For the overlays' footers.
pub fn hints(parts: &[(String, &str)]) -> String {
    parts
        .iter()
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, what)| format!("[{key}] {what}"))
        .collect::<Vec<_>>()
        .join("  ")
}

/// Two actions' first keys as `j/k`, for paired hints; one alone when the
/// other has none.
pub fn pair<A: Actions>(a: A, b: A) -> String {
    [key_of(a), key_of(b)]
        .into_iter()
        .filter(|k| !k.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// `action`'s first key in the map in effect.
pub fn key_of<A: Actions>(action: A) -> String {
    get().first(action)
}

static KEYMAP: OnceLock<Keymap> = OnceLock::new();

/// Put `map` in effect. Only the first call counts; it's made once at
/// startup, before anything reads the map.
pub fn install(map: Keymap) {
    let _ = KEYMAP.set(map);
}

/// The bindings in effect: the installed map, else the defaults.
pub fn get() -> &'static Keymap {
    KEYMAP.get_or_init(Keymap::defaults)
}

#[cfg(test)]
mod tests;
