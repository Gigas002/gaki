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
//! An action left out keeps its default, so an empty or missing file changes
//! nothing. The file is read once at startup, and gaki-tui refuses to start
//! on an unknown section or action, a key it can't parse, a reserved key, or
//! two actions on one key in the same context, naming the line.
//!
//! Some keys can't be rebound, so a bad file can't lock anyone out: `Esc`,
//! `?` (help), the arrow keys and Ctrl+C (quit) always do what they do.

mod key;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ratatui::crossterm::event::KeyCode;

pub use key::Key;

macro_rules! library_actions {
    ($($variant:ident => $name:literal [$($key:literal),*],)*) => {
        /// What a key in the library (no overlay open) does.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum LibraryAction {
            $($variant),*
        }

        impl LibraryAction {
            /// Every action, in the order `keybinds.toml` lists them.
            pub const ALL: &'static [LibraryAction] = &[$(LibraryAction::$variant),*];

            /// The name `keybinds.toml` uses.
            pub fn name(self) -> &'static str {
                match self {
                    $(LibraryAction::$variant => $name),*
                }
            }

            /// The keys it has when the file doesn't say.
            pub fn default_keys(self) -> &'static [&'static str] {
                match self {
                    $(LibraryAction::$variant => &[$($key),*]),*
                }
            }
        }
    };
}

library_actions! {
    Quit => "quit" ["q"],
    Down => "down" ["j"],
    Up => "up" ["k"],
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

/// The sections `keybinds.toml` may have.
const SECTIONS: &[&str] = &["library"];

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

/// The bindings in effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    library: HashMap<Key, LibraryAction>,
    library_keys: BTreeMap<LibraryAction, Vec<Key>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::defaults()
    }
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

impl Keymap {
    /// Every action on its default keys.
    pub fn defaults() -> Self {
        let keys = LibraryAction::ALL
            .iter()
            .map(|&action| {
                let keys = action
                    .default_keys()
                    .iter()
                    .map(|k| Key::parse(k).expect("default keys parse"))
                    .collect();
                (action, keys)
            })
            .collect();
        Self::build(keys, &HashMap::new()).expect("default keys don't clash")
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
        type Section = BTreeMap<String, toml::Spanned<toml::Value>>;
        let file: BTreeMap<String, toml::Spanned<Section>> = toml::from_str(text).map_err(|e| {
            error(
                e.span().map(|s| line_of(text, s.start)),
                e.message().to_string(),
            )
        })?;
        let mut keys: BTreeMap<LibraryAction, Vec<Key>> = LibraryAction::ALL
            .iter()
            .map(|&a| {
                let keys = a.default_keys().iter().filter_map(|k| Key::parse(k).ok());
                (a, keys.collect())
            })
            .collect();
        let mut lines: HashMap<LibraryAction, usize> = HashMap::new();
        for (name, section) in &file {
            let line = line_of(text, section.span().start);
            if !SECTIONS.contains(&name.as_str()) {
                return Err(error(
                    Some(line),
                    format!(
                        "unknown section `[{name}]`; the sections are {}",
                        SECTIONS.join(", ")
                    ),
                ));
            }
            for (action_name, value) in section.get_ref() {
                let line_no = line_of(text, value.span().start);
                let line = Some(line_no);
                let Some(&action) = LibraryAction::ALL
                    .iter()
                    .find(|a| a.name() == action_name.as_str())
                else {
                    return Err(error(
                        line,
                        format!("unknown action `{action_name}` in [{name}]"),
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
                    if !parsed.contains(&key) {
                        parsed.push(key);
                    }
                }
                keys.insert(action, parsed);
                lines.insert(action, line_no);
            }
        }
        Self::build(keys, &lines)
    }

    fn build(
        library_keys: BTreeMap<LibraryAction, Vec<Key>>,
        lines: &HashMap<LibraryAction, usize>,
    ) -> Result<Self, KeymapError> {
        let mut library: HashMap<Key, LibraryAction> = HashMap::new();
        for &action in LibraryAction::ALL {
            for &key in library_keys
                .get(&action)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                if let Some(&other) = library.get(&key) {
                    let line = lines.get(&action).or_else(|| lines.get(&other)).copied();
                    return Err(error(
                        line,
                        format!(
                            "`{key}` is bound to both `{}` and `{}` in [library]; give one of them another key",
                            other.name(),
                            action.name()
                        ),
                    ));
                }
                library.insert(key, action);
            }
        }
        Ok(Self {
            library,
            library_keys,
        })
    }

    /// What `key` does in the library.
    pub fn library_action(&self, key: Key) -> Option<LibraryAction> {
        self.library.get(&key).copied()
    }

    /// The keys bound to `action`.
    pub fn keys(&self, action: LibraryAction) -> &[Key] {
        self.library_keys
            .get(&action)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// `action`'s keys for the help and the hints, `j / ↓` style; empty
    /// when it has none.
    pub fn label(&self, action: LibraryAction) -> String {
        let mut labels: Vec<String> = self.keys(action).iter().map(Key::to_string).collect();
        match action {
            LibraryAction::Down => labels.push("\u{2193}".to_string()),
            LibraryAction::Up => labels.push("\u{2191}".to_string()),
            _ => {}
        }
        labels.join(" / ")
    }

    /// `action`'s first key, for the one-key hints; empty when it has none.
    pub fn first(&self, action: LibraryAction) -> String {
        self.keys(action)
            .first()
            .map(Key::to_string)
            .unwrap_or_default()
    }
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
