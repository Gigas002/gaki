//! `clean`: find gaki's caches and leftovers, with their sizes, and delete
//! the ones asked for.
//!
//! Never touched: the config, the session, the install backlog, installed
//! games, Cloud saves, prefixes, and partial downloads (those stay with
//! `install forget --delete-files`). Cache directories are emptied rather
//! than removed, so a running daemon or TUI can keep writing into them.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use serde::Serialize;

use crate::library::relocate::dir_size;

#[cfg(test)]
mod tests;

/// A kind of thing `clean` can delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Downloaded cover art and artwork.
    Images,
    /// Cached `info` records and the store's tag and category names.
    Info,
    /// The cached list of owned games.
    Library,
    /// gaki-tui's cached library and cover art.
    Tui,
    /// Launch logs, except each game's newest.
    Logs,
    /// Install lock files no process holds.
    Locks,
    /// Shader caches: those of games that aren't installed, or with
    /// [`Selection::all_shaders`], every game's.
    Shaders,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Images,
        Kind::Info,
        Kind::Library,
        Kind::Tui,
        Kind::Logs,
        Kind::Locks,
        Kind::Shaders,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Images => "images",
            Kind::Info => "info",
            Kind::Library => "library",
            Kind::Tui => "tui",
            Kind::Logs => "logs",
            Kind::Locks => "locks",
            Kind::Shaders => "shaders",
        }
    }
}

/// What to look for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The kinds; empty means all of them.
    pub kinds: Vec<Kind>,
    /// Shader caches of installed games too.
    pub all_shaders: bool,
}

impl Selection {
    fn wants(&self, kind: Kind) -> bool {
        self.kinds.is_empty() || self.kinds.contains(&kind)
    }
}

/// One thing to delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub kind: Kind,
    pub path: PathBuf,
    /// The size of the files in it.
    pub bytes: u64,
    /// Empty this directory but keep it.
    #[serde(skip)]
    pub keep_dir: bool,
    /// Which game it belongs to, when it's one game's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<u32>,
}

/// What [`delete`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    pub deleted: Vec<Item>,
    pub freed_bytes: u64,
    /// Paths that couldn't be deleted, with why.
    pub failed: Vec<(PathBuf, String)>,
}

/// Where everything lives. [`Layout::user`] is the real one; tests build
/// their own.
#[derive(Debug, Clone)]
pub struct Layout {
    pub images: PathBuf,
    /// `info_cache/` and the store name tables sit here.
    pub data: PathBuf,
    pub library_cache: PathBuf,
    pub tui_cache: PathBuf,
    pub launch_logs: PathBuf,
    pub locks: PathBuf,
    /// Every directory games' shader caches can be under.
    pub shader_roots: Vec<PathBuf>,
}

impl Layout {
    /// The user's directories, and the shader cache roots that
    /// `config.toml`, the profiles and `games.toml` name.
    pub fn user() -> Result<Self> {
        use crate::core::config;
        let data = config::data_dir()?;
        Ok(Self {
            images: config::image_cache_dir()?,
            library_cache: config::library_cache_path()?,
            tui_cache: config::cache_dir()?.join("tui"),
            launch_logs: config::config_dir()?.join("logs"),
            locks: config::state_dir()?.join("installs"),
            shader_roots: shader_roots()?,
            data,
        })
    }
}

/// The default shader cache root and every one the configuration names.
fn shader_roots() -> Result<Vec<PathBuf>> {
    use crate::config::{Catalog, ShaderCache, expand_tilde};
    let home = crate::core::config::home_dir()?;
    let mut roots = vec![crate::core::config::shader_cache_dir()?];
    let catalog = Catalog::load_user()?;
    let named = std::iter::once(catalog.config.shader_cache.as_ref())
        .chain(catalog.profiles.iter().map(|p| p.shader_cache.as_ref()))
        .chain(catalog.games.iter().map(|g| g.shader_cache.as_ref()));
    for value in named.flatten() {
        if let ShaderCache::Dir(dir) = value {
            let dir = expand_tilde(dir, &home);
            if !roots.contains(&dir) {
                roots.push(dir);
            }
        }
    }
    Ok(roots)
}

/// The apps installed in any Steam library gaki can find.
pub async fn installed_apps() -> HashSet<u32> {
    let roots = crate::library::all_library_roots().await;
    crate::library::scan_installed_in(&roots)
        .await
        .into_keys()
        .collect()
}

/// Everything `selection` asks for in the user's directories.
pub async fn plan(selection: &Selection) -> Result<Vec<Item>> {
    let layout = Layout::user()?;
    let installed = if selection.wants(Kind::Shaders) && !selection.all_shaders {
        installed_apps().await
    } else {
        HashSet::new()
    };
    Ok(plan_in(&layout, selection, &installed, &lock_is_held))
}

#[cfg(feature = "install")]
fn lock_is_held(app_id: u32) -> bool {
    crate::ops::install::lock_is_held(app_id)
}

/// Without installs nothing takes the locks; leave them be anyway.
#[cfg(not(feature = "install"))]
fn lock_is_held(_: u32) -> bool {
    true
}

/// A launch log without a summary this young may belong to a running game.
const RUNNING_LOG_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// [`plan`] over `layout`: `installed` decides which shader caches are
/// leftovers, and `held` which lock files are in use.
pub fn plan_in(
    layout: &Layout,
    selection: &Selection,
    installed: &HashSet<u32>,
    held: &dyn Fn(u32) -> bool,
) -> Vec<Item> {
    let mut items = Vec::new();
    let mut push = |kind, path: PathBuf, keep_dir, app_id| {
        if path.exists() {
            let bytes = if path.is_dir() {
                dir_size(&path)
            } else {
                std::fs::metadata(&path).map_or(0, |m| m.len())
            };
            if !(keep_dir && is_empty_dir(&path)) {
                items.push(Item {
                    kind,
                    path,
                    bytes,
                    keep_dir,
                    app_id,
                });
            }
        }
    };
    if selection.wants(Kind::Images) {
        push(Kind::Images, layout.images.clone(), true, None);
    }
    if selection.wants(Kind::Info) {
        push(Kind::Info, layout.data.join("info_cache"), true, None);
        for table in ["tag_names", "category_names"] {
            push(
                Kind::Info,
                layout.data.join(format!("{table}.toml")),
                false,
                None,
            );
        }
    }
    if selection.wants(Kind::Library) {
        push(Kind::Library, layout.library_cache.clone(), false, None);
    }
    if selection.wants(Kind::Tui) {
        push(Kind::Tui, layout.tui_cache.clone(), true, None);
    }
    if selection.wants(Kind::Logs) {
        for (dir, app_id) in old_launch_logs(&layout.launch_logs) {
            push(Kind::Logs, dir, false, app_id);
        }
    }
    if selection.wants(Kind::Locks) {
        for (path, app_id) in numbered(&layout.locks, ".lock") {
            if path.is_file() && !held(app_id) {
                push(Kind::Locks, path, false, Some(app_id));
            }
        }
    }
    if selection.wants(Kind::Shaders) {
        for root in &layout.shader_roots {
            for (dir, app_id) in numbered(root, "") {
                if is_shader_cache(&dir) && (selection.all_shaders || !installed.contains(&app_id))
                {
                    push(Kind::Shaders, dir, false, Some(app_id));
                }
            }
        }
    }
    items
}

/// Delete `items`. One that fails is reported and the rest still go.
pub fn delete(items: &[Item]) -> Report {
    let mut report = Report::default();
    for item in items {
        let result = if item.keep_dir {
            empty_dir(&item.path)
        } else if item.path.is_dir() {
            std::fs::remove_dir_all(&item.path)
        } else {
            std::fs::remove_file(&item.path)
        };
        match result {
            Ok(()) => {
                report.freed_bytes += item.bytes;
                report.deleted.push(item.clone());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => report.failed.push((item.path.clone(), e.to_string())),
        }
    }
    report
}

/// The bytes of `items`, per kind.
pub fn totals(items: &[Item]) -> BTreeMap<Kind, (usize, u64)> {
    let mut totals = BTreeMap::new();
    for item in items {
        let entry = totals.entry(item.kind).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += item.bytes;
    }
    totals
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

fn empty_dir(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() && !path.is_symlink() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The entries of `dir` named `<app id><suffix>`.
fn numbered(dir: &Path, suffix: &str) -> Vec<(PathBuf, u32)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, u32)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name.to_str()?.strip_suffix(suffix)?.parse::<u32>().ok()?;
            Some((entry.path(), id))
        })
        .collect();
    found.sort();
    found
}

/// Whether `dir` is a shader cache gaki made: it has one of the folders
/// [`crate::config::ShaderCache::create_dirs`] or Mesa makes. A numbered folder a
/// user keeps in the same place is left alone.
fn is_shader_cache(dir: &Path) -> bool {
    // Mesa makes these inside `MESA_SHADER_CACHE_DIR` itself.
    const MESA: [&str; 2] = ["mesa_shader_cache", "mesa_shader_cache_db"];
    !dir.is_symlink()
        && (crate::config::ShaderCache::env(dir)
            .into_iter()
            .any(|(_, path)| path != dir && path.is_dir())
            || MESA.iter().any(|name| dir.join(name).is_dir()))
}

/// Launch log directories to delete, with their game: all but the newest of
/// each game. A directory without a summary is kept while it's young, since
/// the launch may still be running.
fn old_launch_logs(dir: &Path) -> Vec<(PathBuf, Option<u32>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let now = SystemTime::now();
    let mut by_game: BTreeMap<u32, Vec<(u64, PathBuf)>> = BTreeMap::new();
    let mut old = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path.is_symlink() {
            continue;
        }
        match crate::infra::logging::load_launch_summary(&path) {
            Ok(summary) => by_game
                .entry(summary.app_id)
                .or_default()
                .push((summary.timestamp, path)),
            Err(_) => {
                let age = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok());
                if age.is_some_and(|age| age > RUNNING_LOG_AGE) {
                    old.push((path, None));
                }
            }
        }
    }
    for (app_id, mut logs) in by_game {
        logs.sort();
        logs.pop(); // the newest
        old.extend(logs.into_iter().map(|(_, path)| (path, Some(app_id))));
    }
    old.sort();
    old
}
