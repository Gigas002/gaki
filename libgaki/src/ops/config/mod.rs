//! Reading and changing the configuration.
//!
//! Global settings live in `config.toml` and per-game ones in `games.toml`;
//! both are edited in place, keeping the user's comments
//! ([`crate::config::edit`]). [`load`] gives the settings the Steam session,
//! installs and launches read.

use crate::config::edit::{self, TomlFile};
use crate::config::{Catalog, Config, Platform, Severity, SteamIntegration, ValidationContext};
use crate::core::config::{LauncherConfig, ProxyConfig, load_launcher_config};
use crate::core::error::{ErrorKind, TypedError};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// A requested change to one optional setting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Change<T> {
    /// Leave it as it is.
    #[default]
    Keep,
    /// Reset it to the default.
    Clear,
    /// Set it to this value.
    Set(T),
}

impl<T> Change<T> {
    /// The same change with the value mapped.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Change<U> {
        match self {
            Change::Keep => Change::Keep,
            Change::Clear => Change::Clear,
            Change::Set(v) => Change::Set(f(v)),
        }
    }
}

/// The kind of value a `config.toml` key holds, for turning command-line text
/// into TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Text,
    Path,
    Bool,
    Integer,
}

/// The `config.toml` keys, in the order `config show` lists them.
pub const KEYS: &[(&str, KeyKind)] = &[
    ("steam_library", KeyKind::Path),
    ("profiles_dir", KeyKind::Path),
    ("default_profile", KeyKind::Text),
    ("default_platform", KeyKind::Text),
    ("default_proton", KeyKind::Text),
    ("cloud_sync", KeyKind::Bool),
    ("presence", KeyKind::Text),
    ("proxy", KeyKind::Text),
    ("no_proxy", KeyKind::Text),
    ("install_resume", KeyKind::Text),
    ("experimental", KeyKind::Bool),
    ("info_cache_ttl", KeyKind::Integer),
    ("daemon_idle_timeout", KeyKind::Integer),
    ("reauth_sensitive", KeyKind::Bool),
    ("secret_store", KeyKind::Text),
];

fn config_toml_path() -> Result<PathBuf> {
    Ok(crate::core::config::config_dir()?.join(crate::config::CONFIG_FILE))
}

fn games_toml_path() -> Result<PathBuf> {
    Ok(crate::core::config::config_dir()?.join(crate::config::GAMES_FILE))
}

/// `config.toml` as gaki reads it (defaults for unset keys).
pub fn global() -> Result<Config> {
    TomlFile::open(&config_toml_path()?)?.parse()
}

/// The value `key` is set to in `config.toml`, as TOML text; `None` when unset.
pub fn get_global(key: &str) -> Result<Option<String>> {
    key_kind(key)?;
    let file = TomlFile::open(&config_toml_path()?)?;
    Ok(file
        .doc
        .get(key)
        .and_then(|item| item.as_value())
        .map(|value| value.to_string().trim().to_owned()))
}

fn key_kind(key: &str) -> Result<KeyKind> {
    KEYS.iter()
        .find(|(name, _)| *name == key)
        .map(|(_, kind)| *kind)
        .ok_or_else(|| {
            let known: Vec<&str> = KEYS.iter().map(|(name, _)| *name).collect();
            TypedError::new(
                ErrorKind::InvalidInput,
                format!(
                    "unknown config key `{key}`; known keys: {}",
                    known.join(", ")
                ),
            )
            .into()
        })
}

/// `text` as the TOML value for a key of `kind`.
fn to_value(key: &str, kind: KeyKind, text: &str) -> Result<toml_edit::Value> {
    let invalid = |what: &str| {
        anyhow::Error::from(TypedError::new(
            ErrorKind::InvalidInput,
            format!("`{key}` takes {what}, got `{text}`"),
        ))
    };
    Ok(match kind {
        KeyKind::Text | KeyKind::Path => text.into(),
        KeyKind::Bool => text
            .parse::<bool>()
            .map_err(|_| invalid("`true` or `false`"))?
            .into(),
        KeyKind::Integer => {
            let n = text.parse::<u64>().map_err(|_| invalid("a whole number"))?;
            i64::try_from(n)
                .map_err(|_| invalid("a smaller number"))?
                .into()
        }
    })
}

/// Set (`Some`) or unset (`None`) a `config.toml` key. The result must parse
/// and validate without errors, or nothing is written. Returns the new
/// settings and whether the file changed.
pub fn set_global(key: &str, value: Option<&str>) -> Result<(Config, bool)> {
    let kind = key_kind(key)?;
    let value = value.map(|text| to_value(key, kind, text)).transpose()?;
    let mut file = TomlFile::open(&config_toml_path()?)?;
    let changed = file.set(key, value);
    let config: Config = file.parse()?;
    // Checked with the rest of the config dir, so `default_profile` must name
    // a profile that exists.
    let mut catalog = Catalog::load_user()?;
    catalog.config = config.clone();
    let home = crate::core::config::home_dir()?;
    let errors: Vec<String> = catalog
        .validate(&ValidationContext { home: &home })
        .into_iter()
        .filter(|issue| {
            issue.severity == Severity::Error && issue.subject == crate::config::Subject::Config
        })
        .map(|issue| issue.message)
        .collect();
    if !errors.is_empty() {
        return Err(TypedError::new(ErrorKind::InvalidInput, errors.join("; ")).into());
    }
    if changed {
        file.save()?;
    }
    Ok((config, changed))
}

/// Set the Steam presence the daemon announces.
pub fn set_presence(presence: crate::core::config::ChatPresence) -> Result<Config> {
    let name = match presence {
        crate::core::config::ChatPresence::Online => "online",
        crate::core::config::ChatPresence::Offline => "offline",
    };
    Ok(set_global("presence", Some(name))?.0)
}

/// Everything wrong with the config directory: files that didn't load, and
/// the issues in the ones that did.
#[derive(Debug, Default)]
pub struct CheckReport {
    pub load_errors: Vec<String>,
    pub issues: Vec<crate::config::Issue>,
}

impl CheckReport {
    pub fn has_errors(&self) -> bool {
        !self.load_errors.is_empty()
            || self
                .issues
                .iter()
                .any(|issue| issue.severity == Severity::Error)
    }
}

/// Load and validate every config file.
pub fn check() -> Result<CheckReport> {
    let catalog = Catalog::load_user()?;
    let home = crate::core::config::home_dir()?;
    let mut issues = catalog.validate(&ValidationContext { home: &home });
    let config_dir = crate::core::config::config_dir()?;
    issues.extend(leftover_scripts(&config_dir));
    issues.extend(leftover_json(&config_dir));
    Ok(CheckReport {
        load_errors: catalog
            .load_errors
            .iter()
            .map(ToString::to_string)
            .collect(),
        issues,
    })
}

/// A warning for each `<config dir>/scripts/<app id>.sh`, which gaki no longer
/// runs: a game's `wrappers` replaced launch scripts.
pub fn leftover_scripts(config_dir: &std::path::Path) -> Vec<crate::config::Issue> {
    let Ok(entries) = std::fs::read_dir(config_dir.join("scripts")) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "sh"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let app_id = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            crate::config::Issue {
                severity: Severity::Warning,
                subject: crate::config::Subject::File(path.clone()),
                message: format!(
                    "launch scripts are no longer run; add it to the game's `wrappers` in \
                     games.toml instead (`gaki-cli config game {app_id} --wrapper {}`)",
                    path.display()
                ),
            }
        })
        .collect()
}

/// A warning for each old JSON file in `config_dir`, which gaki no longer
/// reads.
pub fn leftover_json(config_dir: &std::path::Path) -> Vec<crate::config::Issue> {
    [
        (
            "config.json",
            "its settings now live in config.toml (`gaki-cli config set`), and version \
             pins in pins.toml",
        ),
        (
            "user_apps.json",
            "per-game settings now live in games.toml (`gaki-cli config game`)",
        ),
        (
            "session.json",
            "tokens now live in the OS keyring; `gaki-cli login` once deletes it",
        ),
        (
            "collections.json",
            "collections now live in collections.toml; `gaki-cli collections pull` \
             fetches them from Steam again, but unpushed local edits are only in the old file",
        ),
        (
            "install_backlog.json",
            "the backlog now lives in install_backlog.toml; start unfinished installs \
             again with `gaki-cli install`",
        ),
    ]
    .into_iter()
    .map(|(name, instead)| (config_dir.join(name), instead))
    .filter(|(path, _)| path.exists())
    .map(|(path, instead)| crate::config::Issue {
        severity: Severity::Warning,
        subject: crate::config::Subject::File(path),
        message: format!("no longer read; {instead}. Delete it once you have moved what you need"),
    })
    .collect()
}

/// One profile, for `profiles list`/`profiles show`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileInfo {
    pub profile: crate::config::Profile,
    /// The profile's file.
    pub path: PathBuf,
    /// Whether it is `default_profile`.
    pub is_default: bool,
    /// The games whose `games.toml` entry names it.
    pub games: Vec<GameRef>,
    /// What's wrong with it.
    #[serde(serialize_with = "issue_strings")]
    pub issues: Vec<crate::config::Issue>,
}

fn issue_strings<S: serde::Serializer>(
    issues: &[crate::config::Issue],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.collect_seq(issues.iter().map(ToString::to_string))
}

/// A game by app id and alias.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameRef {
    pub app_id: u32,
    pub name: Option<String>,
}

impl std::fmt::Display for GameRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{name} ({})", self.app_id),
            None => write!(f, "{}", self.app_id),
        }
    }
}

/// Every profile, sorted by name, and the directory they come from. Profile
/// files that failed to load are returned as errors alongside.
pub fn profiles() -> Result<(PathBuf, Vec<ProfileInfo>, Vec<String>)> {
    let config_dir = crate::core::config::config_dir()?;
    let home = crate::core::config::home_dir()?;
    let catalog = Catalog::load(&config_dir, &home);
    let dir = crate::config::profiles_dir(&config_dir, &catalog.config, &home);
    let ctx = ValidationContext { home: &home };
    let infos = catalog
        .profiles
        .iter()
        .map(|profile| ProfileInfo {
            path: dir.join(format!("{}.toml", profile.name)),
            is_default: catalog.config.default_profile.as_deref() == Some(profile.name.as_str()),
            games: catalog
                .games_using(&profile.name)
                .iter()
                .map(|game| GameRef {
                    app_id: game.app_id,
                    name: game.name.clone(),
                })
                .collect(),
            issues: profile.validate(&ctx),
            profile: profile.clone(),
        })
        .collect();
    let errors = catalog
        .load_errors
        .iter()
        .filter(|e| e.path().starts_with(&dir))
        .map(ToString::to_string)
        .collect();
    Ok((dir, infos, errors))
}

/// One profile by name.
pub fn profile(name: &str) -> Result<ProfileInfo> {
    let (dir, infos, _) = profiles()?;
    infos
        .into_iter()
        .find(|info| info.profile.name == name)
        .ok_or_else(|| {
            TypedError::new(
                ErrorKind::NotFound,
                format!("no profile `{name}` in {}", dir.display()),
            )
            .into()
        })
}

/// Load the launcher configuration (defaults when the file is missing).
pub async fn load() -> Result<LauncherConfig> {
    load_launcher_config().await
}

/// The experimental-features gate.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ExperimentalStatus {
    /// The persisted setting.
    pub experimental: bool,
    /// What actually applies (the setting; kept for drivers that read it).
    pub effective: bool,
}

/// Read, and with `set` change, the experimental-features gate.
pub async fn experimental(set: Option<bool>) -> Result<ExperimentalStatus> {
    let config = match set {
        Some(value) => set_global("experimental", Some(&value.to_string()))?.0,
        None => global()?,
    };
    Ok(ExperimentalStatus {
        experimental: config.experimental,
        effective: config.experimental,
    })
}

/// A change to the network proxy setting.
#[derive(Debug, Clone, Default)]
pub struct ProxyChange {
    /// Drop both the URL and the bypass list (wins over the other fields).
    pub clear: bool,
    /// New proxy URL; an empty string means a direct connection.
    pub url: Option<String>,
    /// New bypass list; an empty string means no exceptions.
    pub no_proxy: Option<String>,
}

impl ProxyChange {
    /// Whether this changes anything.
    pub fn is_noop(&self) -> bool {
        !self.clear && self.url.is_none() && self.no_proxy.is_none()
    }
}

/// Apply `change` to the proxy setting (validating the URL) and return the
/// resulting setting. A no-op change only reads.
pub async fn update_proxy(change: ProxyChange) -> Result<ProxyConfig> {
    let proxy = |config: Config| ProxyConfig {
        url: config.proxy,
        no_proxy: config.no_proxy,
    };
    if change.is_noop() {
        return Ok(proxy(global()?));
    }
    if change.clear {
        set_global("proxy", None)?;
        return Ok(proxy(set_global("no_proxy", None)?.0));
    }
    let mut config = global()?;
    if let Some(url) = change.url {
        let value = url.trim();
        if !value.is_empty() {
            crate::core::net::validate_proxy_url(value)?;
        }
        config = set_global("proxy", (!value.is_empty()).then_some(value))?.0;
    }
    if let Some(no_proxy) = change.no_proxy {
        let value = no_proxy.trim();
        config = set_global("no_proxy", (!value.is_empty()).then_some(value))?.0;
    }
    Ok(proxy(config))
}

/// App ids with a `games.toml` entry.
pub async fn games_with_settings() -> Result<BTreeSet<u32>> {
    let entries: crate::config::GamesFile = TomlFile::open(&games_toml_path()?)?.parse()?;
    Ok(entries.games.iter().map(|game| game.app_id).collect())
}

/// Remove every game's `games.toml` entry. Returns how many there were.
pub async fn clear_all_game_settings() -> Result<usize> {
    let mut games = TomlFile::open(&games_toml_path()?)?;
    let count = edit::clear_games(&mut games.doc);
    if count > 0 {
        games.save()?;
    }
    Ok(count)
}

/// A change to one game's `games.toml` entry.
#[derive(Debug, Clone, Default)]
pub struct GameEntryChange {
    /// Remove the entry; the other fields are ignored.
    pub reset: bool,
    pub name: Change<String>,
    pub profile: Change<String>,
    pub platform: Change<Platform>,
    pub wrappers: Change<Vec<String>>,
    pub steam: Change<SteamIntegration>,
}

impl GameEntryChange {
    fn is_noop(&self) -> bool {
        !self.reset
            && [
                matches!(self.name, Change::Keep),
                matches!(self.profile, Change::Keep),
                matches!(self.platform, Change::Keep),
                matches!(self.wrappers, Change::Keep),
                matches!(self.steam, Change::Keep),
            ]
            .iter()
            .all(|keep| *keep)
    }
}

/// One game's entry, and the settings it resolves to.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameEntry {
    pub app_id: u32,
    /// The `games.toml` entry; `None` when the game has none.
    pub entry: Option<crate::config::Game>,
    /// The profile the game launches with.
    pub profile: Option<String>,
    /// The resolved platform; `None` runs the installed build.
    pub platform: Option<Platform>,
    pub steam: SteamIntegration,
    /// Every wrapper, the profile's first.
    pub wrappers: Vec<String>,
    /// Which platform payloads are installed, as `(linux, windows)`.
    #[serde(skip)]
    pub installed_platforms: Option<(bool, bool)>,
}

/// Apply `change` to `app_id`'s `games.toml` entry and return the result.
/// Returns whether anything changed.
pub async fn update_game_entry(app_id: u32, change: GameEntryChange) -> Result<(GameEntry, bool)> {
    let mut file = TomlFile::open(&games_toml_path()?)?;
    let mut changed = false;
    if change.reset {
        changed |= edit::remove_game(&mut file.doc, app_id);
    } else if !change.is_noop() {
        let table = edit::game_mut(&mut file.doc, app_id);
        let text = |change: Change<String>| match change {
            Change::Keep => None,
            Change::Clear => Some(None),
            Change::Set(v) => Some(Some(toml_edit::Value::from(v))),
        };
        let fields = [
            ("name", text(change.name)),
            ("profile", text(change.profile)),
            (
                "platform",
                text(change.platform.map(|p| p.as_str().to_owned())),
            ),
            ("steam", text(change.steam.map(|s| s.as_str().to_owned()))),
            (
                "wrappers",
                match change.wrappers {
                    Change::Keep => None,
                    Change::Clear => Some(None),
                    Change::Set(list) if list.is_empty() => Some(None),
                    Change::Set(list) => {
                        Some(Some(toml_edit::Value::Array(list.into_iter().collect())))
                    }
                },
            ),
        ];
        for (key, value) in fields {
            if let Some(value) = value {
                changed |= edit::set_in(table, key, value);
            }
        }
        edit::prune_game(&mut file.doc, app_id);
    }

    // Check the whole catalog with the edit applied before writing it.
    let games: crate::config::GamesFile = file.parse()?;
    let mut catalog = Catalog::load_user()?;
    catalog.games = games.games;
    let home = crate::core::config::home_dir()?;
    let errors: Vec<String> = catalog
        .validate(&ValidationContext { home: &home })
        .into_iter()
        .filter(|issue| {
            issue.severity == Severity::Error
                && matches!(&issue.subject, crate::config::Subject::Game { app_id: id, .. } if *id == app_id)
        })
        .map(|issue| issue.message)
        .collect();
    if !errors.is_empty() {
        return Err(TypedError::new(ErrorKind::InvalidInput, errors.join("; ")).into());
    }
    if changed {
        file.save()?;
    }
    let settings = crate::settings::resolve(
        &catalog,
        app_id,
        &crate::settings::Overrides::default(),
        &home,
    )
    .map_err(|e| TypedError::new(ErrorKind::InvalidInput, e.to_string()))?;
    Ok((
        GameEntry {
            app_id,
            entry: catalog.game(app_id).cloned(),
            profile: settings.profile,
            platform: settings.platform,
            steam: settings.steam,
            wrappers: settings.wrappers,
            installed_platforms: installed_platforms(app_id).await,
        },
        changed,
    ))
}

/// Which platform payloads `app_id` has installed: `(linux, windows)`.
async fn installed_platforms(app_id: u32) -> Option<(bool, bool)> {
    let installed = crate::library::scan_installed_app_info().await.ok()?;
    let info = installed.get(&app_id)?;
    crate::library::platform::detect_installed_platform_set(&info.install_path.to_string_lossy())
}

#[cfg(test)]
mod tests;
