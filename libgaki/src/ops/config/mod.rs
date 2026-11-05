//! Reading and changing the configuration.
//!
//! Global settings live in `config.toml` and per-game ones in `games.toml`;
//! both are edited in place, keeping the user's comments
//! ([`crate::config::edit`]). A few settings not in the TOML model yet still
//! live in `config.json` ([`load`]/[`update`]): the keyring gate (replaced in
//! 5.9), the default Proton version, and version pins.

use crate::config::edit::{self, TomlFile};
use crate::config::{Catalog, Config, Platform, Severity, SteamIntegration, ValidationContext};
use crate::core::config::{
    GameConfig, LauncherConfig, ProxyConfig, load_launcher_config, load_user_configs,
    save_launcher_config, save_user_configs,
};
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::DepotPlatform;
use anyhow::{Context, Result};
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

    /// Apply to `slot`; returns whether anything was requested.
    fn apply_to(self, slot: &mut Option<T>) -> bool {
        match self {
            Change::Keep => false,
            Change::Clear => {
                *slot = None;
                true
            }
            Change::Set(v) => {
                *slot = Some(v);
                true
            }
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
    ("cloud_sync", KeyKind::Bool),
    ("presence", KeyKind::Text),
    ("proxy", KeyKind::Text),
    ("no_proxy", KeyKind::Text),
    ("install_resume", KeyKind::Text),
    ("experimental", KeyKind::Bool),
    ("script_dir", KeyKind::Path),
    ("info_cache_ttl", KeyKind::Integer),
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
    Ok(CheckReport {
        load_errors: catalog
            .load_errors
            .iter()
            .map(ToString::to_string)
            .collect(),
        issues: catalog.validate(&ValidationContext { home: &home }),
    })
}

/// Load the launcher configuration (defaults when the file is missing).
pub async fn load() -> Result<LauncherConfig> {
    load_launcher_config().await
}

/// Load, apply `edit`, and save the launcher configuration. Returns the saved
/// configuration. Nothing is written if `edit` fails.
pub async fn update(
    edit: impl FnOnce(&mut LauncherConfig) -> Result<()>,
) -> Result<LauncherConfig> {
    let mut config = load_launcher_config().await?;
    edit(&mut config)?;
    save_launcher_config(&config).await?;
    Ok(config)
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

/// App ids that have any per-game settings, in either store.
pub async fn games_with_settings() -> Result<BTreeSet<u32>> {
    let config = load().await?;
    let user_configs = load_user_configs().await?;
    let entries: crate::config::GamesFile = TomlFile::open(&games_toml_path()?)?.parse()?;
    Ok(config
        .game_configs
        .keys()
        .chain(user_configs.keys())
        .copied()
        .chain(entries.games.iter().map(|game| game.app_id))
        .collect())
}

/// Reset every game's per-game settings (both stores). Returns how many games
/// had settings.
pub async fn clear_all_game_settings() -> Result<usize> {
    let count = games_with_settings().await?.len();
    if count == 0 {
        return Ok(0);
    }
    let mut games = TomlFile::open(&games_toml_path()?)?;
    if edit::clear_games(&mut games.doc) > 0 {
        games.save()?;
    }
    update(|c| {
        c.game_configs.clear();
        Ok(())
    })
    .await
    .context("failed saving game config")?;
    save_user_configs(&Default::default())
        .await
        .context("failed saving per-game config")?;
    Ok(count)
}

/// A change to one game's `games.toml` entry.
#[derive(Debug, Clone, Default)]
pub struct GameEntryChange {
    /// Remove the entry (and the game's `config.json` settings); the other
    /// fields are ignored.
    pub reset: bool,
    pub name: Change<String>,
    pub profile: Change<String>,
    pub platform: Change<Platform>,
    pub script: Change<PathBuf>,
    pub steam: Change<SteamIntegration>,
}

impl GameEntryChange {
    fn is_noop(&self) -> bool {
        !self.reset
            && [
                matches!(self.name, Change::Keep),
                matches!(self.profile, Change::Keep),
                matches!(self.platform, Change::Keep),
                matches!(self.script, Change::Keep),
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
    pub script: Option<PathBuf>,
    /// Which platform payloads are installed, as `(linux, windows)`.
    #[serde(skip)]
    pub installed_platforms: Option<(bool, bool)>,
}

/// Apply `change` to `app_id`'s `games.toml` entry and return the result.
/// Setting anything also drops the game's old `config.json` launch settings
/// (forced Proton, platform, launch script), which would otherwise still win.
/// Returns whether anything changed.
pub async fn update_game_entry(app_id: u32, change: GameEntryChange) -> Result<(GameEntry, bool)> {
    let mut file = TomlFile::open(&games_toml_path()?)?;
    let mut changed = false;
    if change.reset {
        changed |= edit::remove_game(&mut file.doc, app_id);
        let mut user_configs = load_user_configs().await?;
        if user_configs.remove(&app_id).is_some() {
            save_user_configs(&user_configs)
                .await
                .context("failed saving per-game config")?;
            changed = true;
        }
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
            (
                "script",
                text(change.script.map(|p| p.to_string_lossy().into_owned())),
            ),
            ("steam", text(change.steam.map(|s| s.as_str().to_owned()))),
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
    if changed || change.reset {
        let mut config = load().await?;
        if let Some(legacy) = config.game_configs.get_mut(&app_id) {
            let had = legacy.forced_proton_version.take().is_some()
                | legacy.platform_preference.take().is_some()
                | legacy.launch_script.take().is_some();
            if had {
                save_launcher_config(&config)
                    .await
                    .context("failed saving game config")?;
            }
        }
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
            script: crate::launch::launch_script::resolve(app_id, None, None, false),
            installed_platforms: installed_platforms(app_id).await,
        },
        changed,
    ))
}

/// A change to one game's launch settings in `config.json`. The TUI's game
/// settings still use this until they move to [`update_game_entry`].
#[derive(Debug, Clone, Default)]
pub struct GameSettingsChange {
    /// Reset every per-game setting (both stores); the other fields are ignored.
    pub reset: bool,
    pub proton: Change<String>,
    pub platform: Change<DepotPlatform>,
    pub launch_script: Change<PathBuf>,
}

/// One game's launch settings, plus what they fall back to.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameSettings {
    pub app_id: u32,
    pub forced_proton_version: Option<String>,
    pub platform_preference: Option<String>,
    pub launch_script: Option<String>,
    /// The global default Proton version, used when none is forced.
    #[serde(skip)]
    pub default_proton_version: String,
    /// Which platform payloads are installed, as `(linux, windows)`; `None`
    /// when the game isn't installed or its directory can't be read.
    #[serde(skip)]
    pub installed_platforms: Option<(bool, bool)>,
}

/// Apply `change` to `app_id`'s settings and return them. Also returns whether
/// anything was saved.
pub async fn update_game(app_id: u32, change: GameSettingsChange) -> Result<(GameSettings, bool)> {
    let mut config = load().await?;
    let changed = if change.reset {
        // The launch tweaks (env, options, graphics layers) live in a second store.
        let mut user_configs = load_user_configs().await?;
        if user_configs.remove(&app_id).is_some() {
            save_user_configs(&user_configs)
                .await
                .context("failed saving per-game config")?;
        }
        config.game_configs.remove(&app_id).is_some()
    } else {
        let entry = config.game_configs.entry(app_id).or_default();
        let platform = match change.platform {
            Change::Keep => Change::Keep,
            Change::Clear => Change::Clear,
            Change::Set(p) => Change::Set(platform_name(p).to_string()),
        };
        let script = match change.launch_script {
            Change::Keep => Change::Keep,
            Change::Clear => Change::Clear,
            Change::Set(p) => Change::Set(p.to_string_lossy().into_owned()),
        };
        // Evaluate every change; `|` doesn't short-circuit.
        change.proton.apply_to(&mut entry.forced_proton_version)
            | platform.apply_to(&mut entry.platform_preference)
            | script.apply_to(&mut entry.launch_script)
    };
    if changed {
        save_launcher_config(&config)
            .await
            .context("failed saving game config")?;
    }

    let entry: GameConfig = config
        .game_configs
        .get(&app_id)
        .cloned()
        .unwrap_or_default();
    Ok((
        GameSettings {
            app_id,
            forced_proton_version: entry.forced_proton_version,
            platform_preference: entry.platform_preference,
            launch_script: entry.launch_script,
            default_proton_version: config.proton_version,
            installed_platforms: installed_platforms(app_id).await,
        },
        changed,
    ))
}

/// The config spelling of a platform preference.
pub fn platform_name(platform: DepotPlatform) -> &'static str {
    match platform {
        DepotPlatform::Windows => "windows",
        DepotPlatform::Linux => "linux",
    }
}

/// Which platform payloads `app_id` has installed: `(linux, windows)`.
async fn installed_platforms(app_id: u32) -> Option<(bool, bool)> {
    let installed = crate::library::scan_installed_app_info().await.ok()?;
    let info = installed.get(&app_id)?;
    crate::library::platform::detect_installed_platform_set(&info.install_path.to_string_lossy())
}

#[cfg(test)]
mod tests;
