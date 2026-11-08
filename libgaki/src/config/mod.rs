//! The TOML configuration: `config.toml`, `profiles/<name>.toml` and
//! `games.toml`, all under `$XDG_CONFIG_HOME/gaki/`.
//!
//! [`Catalog::load`] reads all three. A missing file is not an error: it means
//! defaults, no profiles, or no game overrides. A file that can't be read or
//! parsed becomes a [`LoadError`] and the rest still loads, so one bad profile
//! doesn't hide the others. [`Catalog::validate`] then reports what's wrong
//! with the contents as [`Issue`]s.
//!
//! Resolving these into the settings of one launch is the next layer up.

pub mod edit;
mod game;
mod global;
mod issue;
mod profile;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub use game::{Game, GamesFile};
pub use global::{Config, SecretStoreKind};
pub use issue::{Issue, Severity, Subject, ValidationContext, expand_tilde, validate_name};
pub use profile::{
    Anticheat, Profile, ProtonSection, Runner, ShaderCache, SteamIntegration, is_supported,
    validate_dll_override, validate_profile,
};

pub const CONFIG_FILE: &str = "config.toml";
pub const GAMES_FILE: &str = "games.toml";
pub const PROFILES_DIR: &str = "profiles";

/// Which build of a game to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

impl From<crate::core::models::DepotPlatform> for Platform {
    fn from(platform: crate::core::models::DepotPlatform) -> Self {
        match platform {
            crate::core::models::DepotPlatform::Linux => Self::Linux,
            crate::core::models::DepotPlatform::Windows => Self::Windows,
        }
    }
}

impl From<Platform> for crate::core::models::DepotPlatform {
    fn from(platform: Platform) -> Self {
        match platform {
            Platform::Linux => Self::Linux,
            Platform::Windows => Self::Windows,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a file could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("could not read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("could not parse {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("{}: `name` is `{name}`, but the file is named `{stem}`", path.display())]
    NameMismatch {
        path: PathBuf,
        name: String,
        stem: String,
    },
}

impl LoadError {
    pub fn path(&self) -> &Path {
        match self {
            Self::Read { path, .. }
            | Self::Parse { path, .. }
            | Self::NameMismatch { path, .. } => path,
        }
    }
}

/// Everything in the config directory.
#[derive(Debug, Default)]
pub struct Catalog {
    pub config: Config,
    /// Sorted by name.
    pub profiles: Vec<Profile>,
    pub games: Vec<Game>,
    pub load_errors: Vec<LoadError>,
}

impl Catalog {
    /// Load the files under `config_dir`. `home` expands a `~` in
    /// `profiles_dir`.
    pub fn load(config_dir: &Path, home: &Path) -> Self {
        let mut load_errors = Vec::new();
        let config: Config =
            read_toml(&config_dir.join(CONFIG_FILE), &mut load_errors).unwrap_or_default();
        let profiles_dir = profiles_dir(config_dir, &config, home);
        let profiles = load_profiles(&profiles_dir, &mut load_errors);
        let games = read_toml::<GamesFile>(&config_dir.join(GAMES_FILE), &mut load_errors)
            .unwrap_or_default()
            .games;
        Self {
            config,
            profiles,
            games,
            load_errors,
        }
    }

    /// Load from gaki's config directory (`$XDG_CONFIG_HOME/gaki`).
    pub fn load_user() -> anyhow::Result<Self> {
        let config_dir = crate::core::config::config_dir()?;
        let home = crate::core::config::home_dir()?;
        Ok(Self::load(&config_dir, &home))
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    pub fn game(&self, app_id: u32) -> Option<&Game> {
        self.games.iter().find(|game| game.app_id == app_id)
    }

    /// The games whose entry names profile `name`.
    pub fn games_using(&self, name: &str) -> Vec<&Game> {
        self.games
            .iter()
            .filter(|game| game.profile.as_deref() == Some(name))
            .collect()
    }

    /// The game whose `name` alias is `alias`.
    pub fn game_by_alias(&self, alias: &str) -> Option<&Game> {
        self.games
            .iter()
            .find(|game| game.name.as_deref() == Some(alias))
    }

    /// The app id `reference` names: an app id, or a game's `name` alias.
    pub fn app_id(&self, reference: &str) -> Result<u32, String> {
        if let Ok(app_id) = reference.parse() {
            return Ok(app_id);
        }
        self.game_by_alias(reference)
            .map(|game| game.app_id)
            .ok_or_else(|| {
                format!("`{reference}` is neither an app id nor a game `name` in {GAMES_FILE}")
            })
    }

    /// Every file's own issues, plus the ones across files: duplicate games,
    /// and references to profiles that don't exist.
    pub fn validate(&self, ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues = self.config.validate(ctx);
        let known: HashSet<&str> = self.profiles.iter().map(|p| p.name.as_str()).collect();
        let missing = |subject: Subject, field: &str, name: &str| Issue {
            severity: Severity::Error,
            subject,
            message: format!("`{field}` names profile `{name}`, which doesn't exist"),
        };

        if let Some(name) = &self.config.default_profile
            && !known.contains(name.as_str())
        {
            issues.push(missing(Subject::Config, "default_profile", name));
        }
        for profile in &self.profiles {
            issues.extend(validate_profile(profile, ctx));
        }

        let mut app_ids = HashSet::new();
        let mut aliases = BTreeMap::new();
        for game in &self.games {
            issues.extend(game.validate(ctx));
            if !app_ids.insert(game.app_id) {
                issues.push(Issue {
                    severity: Severity::Error,
                    subject: game.subject(),
                    message: format!("app id {} has more than one entry", game.app_id),
                });
            }
            if let Some(alias) = &game.name
                && let Some(first) = aliases.insert(alias.as_str(), game.app_id)
            {
                issues.push(Issue {
                    severity: Severity::Error,
                    subject: game.subject(),
                    message: format!("name `{alias}` is already used by app id {first}"),
                });
            }
            if let Some(name) = &game.profile
                && !known.contains(name.as_str())
            {
                issues.push(missing(game.subject(), "profile", name));
            }
        }
        issues
    }
}

/// Where `config`'s profiles live: `profiles_dir`, or `<config dir>/profiles`.
pub fn profiles_dir(config_dir: &Path, config: &Config, home: &Path) -> PathBuf {
    match &config.profiles_dir {
        Some(dir) => expand_tilde(dir, home),
        None => config_dir.join(PROFILES_DIR),
    }
}

/// [`Catalog::app_id`] against the user's config. Plain app ids don't read
/// any file.
pub fn resolve_app_ref(reference: &str) -> Result<u32, String> {
    if let Ok(app_id) = reference.parse() {
        return Ok(app_id);
    }
    Catalog::load_user()
        .map_err(|e| format!("{e:#}"))?
        .app_id(reference)
}

/// Parse `path`, or `None` when it is missing or broken (recording why).
fn read_toml<T: serde::de::DeserializeOwned>(
    path: &Path,
    errors: &mut Vec<LoadError>,
) -> Option<T> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(source) => {
            errors.push(LoadError::Read {
                path: path.to_path_buf(),
                source,
            });
            return None;
        }
    };
    toml::from_str(&text)
        .map_err(|source| {
            errors.push(LoadError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            });
        })
        .ok()
}

/// Every `*.toml` in `dir`, sorted by name. Each profile's `name` must match
/// its file stem, so a profile is always found by its file name.
fn load_profiles(dir: &Path, errors: &mut Vec<LoadError>) -> Vec<Profile> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(source) => {
            errors.push(LoadError::Read {
                path: dir.to_path_buf(),
                source,
            });
            return Vec::new();
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml") && path.is_file())
        .collect();
    paths.sort();

    let mut profiles = Vec::new();
    for path in paths {
        let Some(profile) = read_toml::<Profile>(&path, errors) else {
            continue;
        };
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if profile.name != stem {
            errors.push(LoadError::NameMismatch {
                path,
                name: profile.name,
                stem,
            });
            continue;
        }
        profiles.push(profile);
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}
