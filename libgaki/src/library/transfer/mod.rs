//! Library files for `library export` and `library import`: the games of a
//! library as `[[game]]` entries, to rebuild it on another machine.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Platform;
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::LibraryGame;

/// Top of an exported file.
const HEADER: &str = "# gaki library: `gaki-cli library import <file>` installs every game\n\
                      # below that isn't installed yet.\n\n";

/// A library file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryFile {
    #[serde(default, rename = "game")]
    pub games: Vec<LibraryEntry>,
}

/// One game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub app_id: u32,
    /// For people reading the file; import doesn't need it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The build to install. Unset picks it as a plain `install` would.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    /// The beta branch it was on; unset is the default (`public`) branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// The library as a file: the installed games, or with `all` every game in
/// it, sorted by name. Installed games carry their platform and branch.
pub fn export(games: &[LibraryGame], all: bool) -> LibraryFile {
    let mut entries: Vec<LibraryEntry> = games
        .iter()
        .filter(|g| all || g.is_installed)
        .map(|g| {
            let installed = g.is_installed;
            LibraryEntry {
                app_id: g.app_id,
                name: g.name.clone(),
                platform: g
                    .platform
                    .as_deref()
                    .filter(|_| installed)
                    .and_then(platform_of),
                branch: Some(g.active_branch.clone())
                    .filter(|b| installed && !b.is_empty() && b != "public"),
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.app_id.cmp(&b.app_id))
    });
    LibraryFile { games: entries }
}

/// The build gaki installs for an on-disk platform; none for macOS.
fn platform_of(name: &str) -> Option<Platform> {
    match name {
        "windows" => Some(Platform::Windows),
        "linux" => Some(Platform::Linux),
        _ => None,
    }
}

/// The file's text.
pub fn to_toml(file: &LibraryFile) -> Result<String> {
    Ok(format!(
        "{HEADER}{}",
        toml::to_string(file).context("failed serializing the library")?
    ))
}

/// Parse a library file. An app id may appear only once.
pub fn parse(text: &str) -> Result<LibraryFile> {
    let file: LibraryFile = toml::from_str(text)?;
    let mut seen = HashSet::new();
    for entry in &file.games {
        if entry.app_id == 0 {
            bail!("app id 0 is not a game");
        }
        if !seen.insert(entry.app_id) {
            bail!("app {} is listed twice", entry.app_id);
        }
    }
    Ok(file)
}

/// Read and parse the file at `path`.
/// A missing file is `not_found` and a bad one `invalid_input`.
pub fn read(path: &Path) -> Result<LibraryFile> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        let kind = if e.kind() == std::io::ErrorKind::NotFound {
            ErrorKind::NotFound
        } else {
            ErrorKind::Unknown
        };
        TypedError::new(kind, format!("failed reading {}: {e}", path.display()))
    })?;
    parse(&text).map_err(|e| {
        TypedError::new(
            ErrorKind::InvalidInput,
            format!("failed parsing {}: {e:#}", path.display()),
        )
        .into()
    })
}

/// What an import does with each entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    /// Not installed: these get installed.
    pub install: Vec<LibraryEntry>,
    /// Already installed: skipped.
    pub installed: Vec<LibraryEntry>,
    /// Not in this account's library: skipped.
    pub not_owned: Vec<LibraryEntry>,
}

impl ImportPlan {
    /// Entries to install that were on a beta branch. Installs take the
    /// default branch; these need `set-branch` and `update` afterwards.
    pub fn on_a_branch(&self) -> impl Iterator<Item = &LibraryEntry> {
        self.install.iter().filter(|e| e.branch.is_some())
    }
}

/// Sort a file's entries against the library. An entry without a name takes
/// the library's.
pub fn plan(file: &LibraryFile, games: &[LibraryGame]) -> ImportPlan {
    let mut plan = ImportPlan::default();
    for entry in &file.games {
        let mut entry = entry.clone();
        match games.iter().find(|g| g.app_id == entry.app_id) {
            None => plan.not_owned.push(entry),
            Some(game) => {
                if entry.name.is_empty() {
                    entry.name = game.name.clone();
                }
                if game.is_installed {
                    plan.installed.push(entry);
                } else {
                    plan.install.push(entry);
                }
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests;
