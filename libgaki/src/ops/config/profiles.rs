//! Creating, editing and deleting profiles.
//!
//! Edits go through [`TomlFile`] like `games.toml`'s, so a hand-written
//! profile keeps its comments and layout. A change is checked before it is
//! written: the file must still load, and it must not gain an error it
//! didn't have. gaki only edits the profiles it runs (`proton`, `native`);
//! lootrs' and sruman's are theirs to edit.

use std::path::{Path, PathBuf};

use anyhow::Result;
use toml_edit::{Item, Table, Value};

use super::{Change, EnvChange, ProfileInfo, list, profile};
use crate::config::edit::{self, TomlFile};
use crate::config::{
    Catalog, Profile, Runner, Severity, ShaderCache, SteamIntegration, ValidationContext,
    is_supported, validate_name, validate_profile,
};
use crate::core::error::{ErrorKind, TypedError};

/// A change to a profile. A new profile starts from `name` and `runner`
/// and gets the rest from here.
#[derive(Debug, Clone, Default)]
pub struct ProfileChange {
    /// `proton` or `native`.
    pub runner: Option<Runner>,
    /// As written; `~` stays `~`.
    pub prefix: Change<String>,
    /// `[proton].build`: an official build name or a path.
    pub proton_build: Change<String>,
    pub steam: Change<SteamIntegration>,
    pub wrappers: Change<Vec<String>>,
    pub runner_args: Change<Vec<String>>,
    pub debug: Option<bool>,
    pub env: EnvChange,
    /// `dll_overrides`, edited like `env` (`winhttp` → `n,b`).
    pub dll_overrides: EnvChange,
    pub shader_cache: Change<ShaderCache>,
}

fn invalid(message: impl Into<String>) -> anyhow::Error {
    TypedError::new(ErrorKind::InvalidInput, message.into()).into()
}

/// The profiles directory and the catalog it was found from.
fn location() -> Result<(Catalog, PathBuf, PathBuf)> {
    let config_dir = crate::core::config::config_dir()?;
    let home = crate::core::config::home_dir()?;
    let catalog = Catalog::load(&config_dir, &home);
    let dir = crate::config::profiles_dir(&config_dir, &catalog.config, &home);
    Ok((catalog, dir, home))
}

fn check_runner(runner: Runner) -> Result<()> {
    match runner {
        Runner::Proton | Runner::Native => Ok(()),
        other => Err(invalid(format!(
            "gaki makes `proton` and `native` profiles, not `{other}`"
        ))),
    }
}

/// Refuses profiles gaki doesn't run.
fn check_owned(profile: &Profile) -> Result<()> {
    if is_supported(profile) {
        return Ok(());
    }
    let tool = if profile.runner == Runner::Slr {
        "sruman"
    } else {
        "lootrs"
    };
    Err(invalid(format!(
        "profile `{}` runs `{}`, a {tool} runner; edit it with {tool} or by hand",
        profile.name, profile.runner
    )))
}

/// Applies `change` to a profile document. Returns whether it changed.
pub(crate) fn apply(table: &mut Table, change: &ProfileChange) -> bool {
    let text = |change: &Change<String>| match change {
        Change::Keep => None,
        Change::Clear => Some(None),
        Change::Set(v) => Some(Some(Value::from(v.as_str()))),
    };
    let mut changed = false;
    let fields = [
        (
            "runner",
            change
                .runner
                .map(|runner| Some(Value::from(runner.as_str()))),
        ),
        ("prefix", text(&change.prefix)),
        (
            "steam",
            text(&change.steam.clone().map(|s| s.as_str().to_owned())),
        ),
        ("wrappers", list(change.wrappers.clone())),
        ("runner_args", list(change.runner_args.clone())),
        (
            "debug",
            change.debug.map(|debug| debug.then(|| Value::from(true))),
        ),
        (
            "shader_cache",
            text(&change.shader_cache.clone().map(|c| c.as_text())),
        ),
    ];
    for (key, value) in fields {
        if let Some(value) = value {
            changed |= edit::set_in(table, key, value);
        }
    }
    if let Some(build) = text(&change.proton_build) {
        let mut proton = match table.remove("proton") {
            Some(Item::Table(proton)) => proton,
            Some(Item::Value(Value::InlineTable(inline))) => inline.into_table(),
            _ => Table::new(),
        };
        changed |= edit::set_in(&mut proton, "build", build);
        if !proton.is_empty() {
            table.insert("proton", Item::Table(proton));
        }
    }
    let env = &change.env;
    changed |= edit::edit_map(table, "env", env.clear, &env.unset, &env.set);
    let dll = &change.dll_overrides;
    changed |= edit::edit_map(table, "dll_overrides", dll.clear, &dll.unset, &dll.set);
    changed
}

/// The edited file as a profile, refused when it doesn't load or gains an
/// error.
fn checked(file: &TomlFile, name: &str, before: &[String], home: &Path) -> Result<Profile> {
    let profile: Profile = file.parse()?;
    if profile.name != name {
        return Err(invalid(format!(
            "`name` must stay `{name}`, the file's name"
        )));
    }
    check_runner(profile.runner)?;
    let new: Vec<String> = errors(&profile, home)
        .into_iter()
        .filter(|error| !before.contains(error))
        .collect();
    if !new.is_empty() {
        return Err(invalid(new.join("; ")));
    }
    Ok(profile)
}

fn errors(profile: &Profile, home: &Path) -> Vec<String> {
    validate_profile(profile, &ValidationContext { home })
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| issue.message)
        .collect()
}

/// Creates `profiles/<name>.toml` for `runner`, with `change` applied.
pub fn create_profile(name: &str, runner: Runner, change: &ProfileChange) -> Result<ProfileInfo> {
    validate_name(name).map_err(|e| invalid(format!("profile name: {e}")))?;
    check_runner(runner)?;
    let (_, dir, home) = location()?;
    let path = dir.join(format!("{name}.toml"));
    if path.exists() {
        return Err(invalid(format!(
            "profile `{name}` exists already: {}",
            path.display()
        )));
    }
    let mut file = TomlFile::open(&path)?;
    file.set("name", Some(Value::from(name)));
    file.set("runner", Some(Value::from(runner.as_str())));
    let change = ProfileChange {
        runner: None,
        ..change.clone()
    };
    apply(file.doc.as_table_mut(), &change);
    checked(&file, name, &[], &home)?;
    file.save()?;
    profile(name)
}

/// Applies `change` to an existing profile. Returns the profile and whether
/// the file changed.
pub fn update_profile(name: &str, change: &ProfileChange) -> Result<(ProfileInfo, bool)> {
    let info = profile(name)?;
    check_owned(&info.profile)?;
    if let Some(runner) = change.runner {
        check_runner(runner)?;
    }
    let (_, _, home) = location()?;
    let before = errors(&info.profile, &home);
    let mut file = TomlFile::open(&info.path)?;
    let changed = apply(file.doc.as_table_mut(), change);
    checked(&file, name, &before, &home)?;
    if changed {
        file.save()?;
    }
    Ok((profile(name)?, changed))
}

/// Deletes a profile's file (its prefix stays). Refused while games or
/// `default_profile` name it, unless `force`. Returns the deleted profile.
pub fn delete_profile(name: &str, force: bool) -> Result<ProfileInfo> {
    let info = profile(name)?;
    check_owned(&info.profile)?;
    if !force {
        let mut users: Vec<String> = info.games.iter().map(ToString::to_string).collect();
        if info.is_default {
            users.insert(0, "default_profile".into());
        }
        if !users.is_empty() {
            return Err(TypedError::new(
                ErrorKind::InvalidInput,
                format!("profile `{name}` is in use by {}", users.join(", ")),
            )
            .into());
        }
    }
    std::fs::remove_file(&info.path)
        .map_err(|e| anyhow::anyhow!("could not delete {}: {e}", info.path.display()))?;
    Ok(info)
}
