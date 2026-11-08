//! Where a library keeps its games and its records.
//!
//! A Steam library keeps both under `steamapps/`: each game in
//! `steamapps/common/<installdir>`, with the appmanifests, Workshop content
//! and Proton prefixes beside them. gaki uses that layout wherever Steam does,
//! so the Steam client sees the same games.
//!
//! A configured `steam_library` that Steam doesn't know (it has no
//! `steamapps/`) gets gaki's own layout instead: each game directly in
//! `<library>/<installdir>`, and the records in
//! `<state dir>/libraries/<escaped library path>/`, laid out like a
//! `steamapps/` (appmanifests, `workshop/`, `compatdata/`). A library's layout
//! is chosen at its first install ([`Layout::for_install`]), and the records
//! directory existing is what remembers it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// One library's layout. [`Layout::steamapps`] is where its records are, the
/// `steamapps/` of a Steam library; [`Layout::common`] is where its games are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
    steamapps: PathBuf,
    flat: bool,
}

impl Layout {
    /// The layout of library `root`: gaki's own when it has records in the
    /// state dir, else Steam's.
    pub fn of(root: &Path) -> Self {
        match records_dir(root) {
            Some(records) if records.is_dir() => Self::flat(root, records),
            _ => Self::steam(root),
        }
    }

    /// The layout an install into `root` uses. Like [`Layout::of`], except
    /// that `configured` (the `steam_library` setting) gets gaki's own layout
    /// when Steam doesn't know it, which this records by creating its records
    /// directory.
    pub fn for_install(root: &Path, configured: &Path) -> Result<Self> {
        let layout = Self::of(root);
        if layout.flat || root.join("steamapps").exists() || !same_path(root, configured) {
            return Ok(layout);
        }
        let records = records_dir(root).context("could not determine gaki's state directory")?;
        std::fs::create_dir_all(&records)
            .with_context(|| format!("could not create {}", records.display()))?;
        Ok(Self::flat(root, records))
    }

    /// The layout whose records directory is `steamapps`: the directory an
    /// appmanifest is in.
    pub fn from_steamapps(steamapps: &Path) -> Option<Self> {
        let parent = steamapps.parent()?;
        if let Ok(base) = crate::core::config::library_records_dir()
            && parent == base
        {
            let root = unescape(steamapps.file_name()?.to_str()?);
            return Some(Self::flat(&root, steamapps.to_path_buf()));
        }
        Some(Self::steam(parent))
    }

    /// The layout of the library a game installed in `game_dir` is in.
    pub fn from_game_dir(game_dir: &Path) -> Option<Self> {
        let common = game_dir.parent()?;
        let layout = Self::of(common);
        if layout.flat {
            return Some(layout);
        }
        Some(Self::steam(common.parent()?.parent()?))
    }

    fn steam(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            steamapps: root.join("steamapps"),
            flat: false,
        }
    }

    fn flat(root: &Path, records: PathBuf) -> Self {
        Self {
            root: root.to_path_buf(),
            steamapps: records,
            flat: true,
        }
    }

    /// The library itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the records are: appmanifests, `workshop/`, `compatdata/`.
    pub fn steamapps(&self) -> &Path {
        &self.steamapps
    }

    /// Whether this is gaki's own layout, which Steam doesn't know.
    pub fn is_flat(&self) -> bool {
        self.flat
    }

    /// The directory the games' folders are in.
    pub fn common(&self) -> PathBuf {
        if self.flat {
            self.root.clone()
        } else {
            self.steamapps.join("common")
        }
    }

    /// A game's folder, from its `installdir`.
    pub fn game_dir(&self, installdir: &str) -> PathBuf {
        self.common().join(installdir)
    }

    /// `appmanifest_<app_id>.acf`.
    pub fn manifest(&self, app_id: u32) -> PathBuf {
        self.steamapps.join(format!("appmanifest_{app_id}.acf"))
    }

    /// `compatdata/<app_id>`: the game's Proton compat data.
    pub fn compatdata(&self, app_id: u32) -> PathBuf {
        self.steamapps.join("compatdata").join(app_id.to_string())
    }
}

/// Where `root`'s records would be in gaki's own layout.
fn records_dir(root: &Path) -> Option<PathBuf> {
    let base = crate::core::config::library_records_dir().ok()?;
    Some(base.join(escape(&canonical(root))))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    canonical(a) == canonical(b)
}

/// A path as one file name, the way `systemd-escape --path` does it:
/// `/home/me/my-games` is `home-me-my\x2dgames`. [`unescape`] reverses it.
fn escape(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::new();
    for (i, byte) in text.trim_matches('/').bytes().enumerate() {
        match byte {
            b'/' => out.push('-'),
            b'.' if i > 0 => out.push('.'),
            b if b.is_ascii_alphanumeric() || b == b'_' => out.push(b as char),
            b => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out
}

fn unescape(name: &str) -> PathBuf {
    let mut bytes = vec![b'/'];
    let raw = name.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            b'-' => {
                bytes.push(b'/');
                i += 1;
            }
            b'\\' if raw.get(i + 1) == Some(&b'x') => {
                match name
                    .get(i + 2..i + 4)
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                {
                    Some(byte) => {
                        bytes.push(byte);
                        i += 4;
                    }
                    None => {
                        bytes.push(b'\\');
                        i += 1;
                    }
                }
            }
            b => {
                bytes.push(b);
                i += 1;
            }
        }
    }
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests;
