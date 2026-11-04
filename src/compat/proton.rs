//! Proton runtime discovery and the official-Proton catalogue.
//!
//! gaki only *installs* official Valve Proton, which ships as free Steam apps and
//! goes through the normal content pipeline ([`crate::steam_client`]) into
//! `steamapps/common`. Custom builds (GE, CachyOS, ...) are not downloaded; when
//! present they are discovered from the `compatibilitytools.d` directories.
//!
//! Discovery order (first hit wins for a given name):
//!
//! 1. system-wide `/usr/share/steam/compatibilitytools.d` (distro packages);
//! 2. the user's `~/.steam/root/compatibilitytools.d` (and the detected Steam
//!    root's, if that differs);
//! 3. Steam-installed official Proton under `<library>/steamapps/common`.
//!
//! The "global default" Proton lives in
//! [`crate::core::config::LauncherConfig::proton_version`]; a fresh install sets it.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// System-wide compatibility tools directory, populated by distro packages.
pub const SYSTEM_COMPAT_TOOLS_DIR: &str = "/usr/share/steam/compatibilitytools.d";

/// An official Proton build that can be installed through Steam.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProtonPackage {
    /// The name used to install/select it (e.g. `Proton 9.0`). Matches Steam's
    /// on-disk install directory under `steamapps/common`.
    pub name: String,
    /// The Steam app id it installs as.
    pub app_id: u32,
}

/// A runtime already present on disk.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledProton {
    pub name: String,
    /// `system` (`/usr/share/...`), `user` (`~/.steam/root/...`) or `steam`
    /// (`steamapps/common`).
    pub location: &'static str,
    pub path: PathBuf,
}

/// Curated set of official Valve Proton apps (name → Steam app id). These are free
/// tools licensed to every account, installed like any other app.
const VALVE_PROTONS: &[(&str, u32)] = &[
    // Names must match Steam's on-disk install directory under steamapps/common so
    // a selected runtime resolves at launch (Experimental uses a dashed dir name).
    ("Proton - Experimental", 1493710),
    ("Proton 9.0", 2805730),
    ("Proton 8.0", 2348590),
    ("Proton 7.0", 1887720),
    ("Proton 6.3", 1580130),
    ("Proton 5.13", 1420170),
];

// ---------------------------------------------------------------------------
// Modern unified-layout component discovery (shared constants)
// ---------------------------------------------------------------------------
//
// Modern runners (Proton 11+, GE, Proton-CachyOS) use a unified layout where
// graphics components live under `<lib-root>/wine/<component>/<arch>` alongside the
// bare base dirs that hold the WOW64 unix-bridge `.so` libraries. These constants
// are the single source of truth shared by `utils`, `dll_provider_resolver`, and the
// `wine_tkg` runner so discovery paths stay consistent and de-duplicated.

/// Lib roots a runner may use. The `*/wine` roots host the arch-split component
/// folders; the bare base dirs (`files/lib`, `files/lib64`) hold the WOW64 unix-bridge
/// `.so` loaders that a modern WOW64 build needs on `WINEDLLPATH`.
pub(crate) const UNIFIED_LIB_SUBDIRS: &[&str] = &[
    "lib/wine",
    "lib64/wine",
    "files/lib/wine",
    "files/lib64/wine",
    "dist/lib/wine",
    "dist/lib64/wine",
    // Bare base dirs — WOW64 unix bridge `.so` libs (ntdll.so, etc.).
    "files/lib",
    "files/lib64",
];

/// Architecture subdirectories a component may split into. The `-windows` dirs hold
/// the PE DLLs; the `-unix` dirs are the WOW64 bridge dirs (host-side `.so`).
pub(crate) const ARCH_SUBDIRS: &[&str] =
    &["x86_64-windows", "i386-windows", "x86_64-unix", "i386-unix"];

/// Graphics component families discovered inside a runner.
pub(crate) const COMPONENT_FAMILIES: &[&str] = &["dxvk", "vkd3d", "vkd3d-proton", "nvapi"];

/// The `<lib-root>/wine/<family>` component directories across every unified/legacy
/// lib layout (excludes the bare base dirs, which never host components).
pub(crate) fn wine_component_dirs(family: &str) -> Vec<String> {
    UNIFIED_LIB_SUBDIRS
        .iter()
        .filter(|l| l.ends_with("wine"))
        .map(|l| format!("{l}/{family}"))
        .collect()
}

/// All candidate relative dirs where a component's DLLs may live: every
/// `<lib-root>/wine/<family>/<arch>` plus the bare `<lib-root>/wine/<family>`
/// (arch-neutral / legacy split-lib layouts).
pub(crate) fn component_dll_subdirs(family: &str) -> Vec<String> {
    let mut out = Vec::new();
    for lib in UNIFIED_LIB_SUBDIRS.iter().filter(|l| l.ends_with("wine")) {
        for arch in ARCH_SUBDIRS {
            out.push(format!("{lib}/{family}/{arch}"));
        }
        out.push(format!("{lib}/{family}"));
    }
    out
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// The detected Steam root's `compatibilitytools.d`, falling back to the
/// conventional `~/.local/share/Steam/compatibilitytools.d`.
pub fn compat_tools_dir() -> Result<PathBuf> {
    if let Some(steam) = crate::core::config::detect_steam_path() {
        return Ok(steam.join("compatibilitytools.d"));
    }
    let home = crate::core::config::home_dir()?;
    Ok(home.join(".local/share/Steam/compatibilitytools.d"))
}

/// Directories searched for Proton runtimes, in priority order, each with its
/// location label. Duplicates (e.g. `~/.steam/root` symlinking to the detected
/// Steam root) are dropped.
pub fn search_dirs(steam_library_path: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut user = Vec::new();
    if let Ok(home) = crate::core::config::home_dir() {
        user.push(home.join(".steam/root/compatibilitytools.d"));
    }
    if let Ok(dir) = compat_tools_dir() {
        user.push(dir);
    }
    search_dirs_from(
        Path::new(SYSTEM_COMPAT_TOOLS_DIR),
        &user,
        steam_library_path,
    )
}

/// [`search_dirs`] with explicit roots, so the ordering is testable.
pub(crate) fn search_dirs_from(
    system: &Path,
    user: &[PathBuf],
    steam_library_path: &Path,
) -> Vec<(PathBuf, &'static str)> {
    let mut dirs = vec![(system.to_path_buf(), "system")];
    dirs.extend(user.iter().map(|d| (d.clone(), "user")));
    dirs.push((steam_library_path.join("steamapps/common"), "steam"));

    let mut seen = std::collections::HashSet::new();
    dirs.retain(|(d, _)| seen.insert(std::fs::canonicalize(d).unwrap_or_else(|_| d.clone())));
    dirs
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Scan disk for installed Proton runtimes across [`search_dirs`].
pub fn list_installed(steam_library_path: &Path) -> Vec<InstalledProton> {
    list_installed_in(&search_dirs(steam_library_path))
}

/// [`list_installed`] over explicit search dirs. Every directory under a
/// `compatibilitytools.d` counts; under `steamapps/common` only directories
/// whose name contains `proton`. When two dirs hold the same name, the one
/// earlier in the search order wins.
pub(crate) fn list_installed_in(dirs: &[(PathBuf, &'static str)]) -> Vec<InstalledProton> {
    let mut out = Vec::new();
    for (dir, location) in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut found: Vec<InstalledProton> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| InstalledProton {
                name: e.file_name().to_string_lossy().to_string(),
                location,
                path: e.path(),
            })
            .filter(|p| *location != "steam" || p.name.to_ascii_lowercase().contains("proton"))
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        out.append(&mut found);
    }

    // Drop the same physical directory reached twice, then shadowed names
    // (the stable sort keeps the earliest search-order entry first).
    let mut seen_paths = std::collections::HashSet::new();
    out.retain(|p| {
        let key = std::fs::canonicalize(&p.path).unwrap_or_else(|_| p.path.clone());
        seen_paths.insert(key)
    });
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out.dedup_by(|a, b| a.name == b.name);
    out
}

// ---------------------------------------------------------------------------
// Official catalogue
// ---------------------------------------------------------------------------

/// Every official Proton build gaki can install.
pub fn official_protons() -> Vec<ProtonPackage> {
    VALVE_PROTONS
        .iter()
        .map(|(name, app_id)| ProtonPackage {
            name: name.to_string(),
            app_id: *app_id,
        })
        .collect()
}

/// Look up an official Proton build by name (case- and punctuation-insensitive,
/// so `Proton Experimental` finds `Proton - Experimental`).
pub fn find_official(name: &str) -> Option<ProtonPackage> {
    let needle = normalize_name(name);
    official_protons()
        .into_iter()
        .find(|p| normalize_name(&p.name) == needle)
}

/// Normalize a runtime name for tolerant matching: lowercase, alphanumerics only.
fn normalize_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
#[path = "proton_tests.rs"]
mod tests;
