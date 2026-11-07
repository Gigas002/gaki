//! Depot browser: list the files of a depot manifest and download one.
//!
//! A manifest is named by id, or by branch (its current manifest there,
//! from PICS). Downloads land flat in an output directory, under the file's
//! own name, and never replace an existing file unless asked.

use anyhow::{Context, Result, anyhow, bail};
use std::path::PathBuf;

pub use crate::library::depot_browser::{EntryKind, ManifestFileEntry, download_target};
use crate::steam_client::{DepotManifestInfo, SteamClient};

/// Which manifest of a depot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestRef {
    /// The current manifest on this branch.
    Branch(String),
    /// A specific manifest id.
    Id(u64),
}

impl Default for ManifestRef {
    fn default() -> Self {
        Self::Branch("public".to_string())
    }
}

impl ManifestRef {
    /// From the CLI's `--manifest` / `--branch`: an id wins, then a branch,
    /// then `public`.
    pub fn from_options(manifest: Option<u64>, branch: Option<&str>) -> Self {
        match (manifest, branch) {
            (Some(id), _) => Self::Id(id),
            (None, Some(branch)) => Self::Branch(branch.to_string()),
            (None, None) => Self::default(),
        }
    }
}

/// The manifest id `reference` names among PICS's current manifests.
pub(crate) fn pick_manifest(
    manifests: &[DepotManifestInfo],
    depot_id: u32,
    reference: &ManifestRef,
) -> Result<u64> {
    let branch = match reference {
        ManifestRef::Id(id) => return Ok(*id),
        ManifestRef::Branch(branch) => branch,
    };
    let depot: Vec<_> = manifests
        .iter()
        .filter(|m| m.depot_id == depot_id)
        .collect();
    if depot.is_empty() {
        bail!(
            "depot {depot_id} has no manifests in this app (check `gaki-cli depots` for its depots)"
        );
    }
    depot
        .iter()
        .find(|m| m.branch.eq_ignore_ascii_case(branch))
        .map(|m| m.manifest_id)
        .ok_or_else(|| {
            let branches: Vec<_> = depot.iter().map(|m| m.branch.as_str()).collect();
            anyhow!(
                "depot {depot_id} has no manifest on branch {branch:?} (branches: {})",
                branches.join(", ")
            )
        })
}

/// The manifest id `reference` names (one PICS request for a branch).
pub async fn resolve_manifest(
    client: &SteamClient,
    app_id: u32,
    depot_id: u32,
    reference: &ManifestRef,
) -> Result<u64> {
    if let ManifestRef::Id(id) = reference {
        return Ok(*id);
    }
    let manifests = client
        .list_depot_manifests(app_id)
        .await
        .with_context(|| format!("failed listing manifests for app {app_id}"))?;
    pick_manifest(&manifests, depot_id, reference)
}

/// Whether `path` matches the glob `pattern`: `*` is any run of characters
/// (`/` included), `?` one character. Case-insensitive, since most depots
/// come from Windows builds.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let path: Vec<char> = path.to_lowercase().chars().collect();
    let (mut p, mut s) = (0, 0);
    // Where the last `*` was, and the path position it matched up to.
    let mut star: Option<(usize, usize)> = None;
    while s < path.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == path[s]) {
            p += 1;
            s += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, s));
            p += 1;
        } else if let Some((star_p, star_s)) = star {
            p = star_p + 1;
            s = star_s + 1;
            star = Some((star_p, star_s + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// The files of one depot manifest.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DepotFiles {
    pub app_id: u32,
    pub depot_id: u32,
    pub manifest_id: u64,
    pub files: Vec<ManifestFileEntry>,
}

impl DepotFiles {
    /// Total size of the regular files.
    pub fn total_size(&self) -> u64 {
        self.files
            .iter()
            .filter(|f| f.kind == EntryKind::File)
            .map(|f| f.size)
            .sum()
    }
}

/// List a depot manifest's entries, keeping those whose path matches
/// `filter` (a glob, see [`glob_match`]) when one is given.
pub async fn files(
    client: &SteamClient,
    app_id: u32,
    depot_id: u32,
    manifest: &ManifestRef,
    filter: Option<&str>,
) -> Result<DepotFiles> {
    let manifest_id = resolve_manifest(client, app_id, depot_id, manifest).await?;
    let mut files = client
        .depot_manifest_files(app_id, depot_id, manifest_id)
        .await?;
    if let Some(pattern) = filter {
        files.retain(|f| glob_match(pattern, &f.path));
    }
    Ok(DepotFiles {
        app_id,
        depot_id,
        manifest_id,
        files,
    })
}

/// A request to download one depot file.
#[derive(Debug, Clone)]
pub struct FileDownload {
    pub app_id: u32,
    pub depot_id: u32,
    pub manifest: ManifestRef,
    /// The file's path inside the depot.
    pub path: String,
    /// Directory to save into (created if missing).
    pub out_dir: PathBuf,
    /// Replace an existing file.
    pub force: bool,
}

/// A finished download.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadedFile {
    pub manifest_id: u64,
    pub path: PathBuf,
    pub size: u64,
}

/// Download one file of a depot to `out_dir/<file name>`, checked against
/// the manifest's SHA-1.
pub async fn download(client: &SteamClient, request: &FileDownload) -> Result<DownloadedFile> {
    let dest = download_target(&request.out_dir, &request.path)?;
    std::fs::create_dir_all(&request.out_dir)
        .with_context(|| format!("failed creating {}", request.out_dir.display()))?;
    let manifest_id =
        resolve_manifest(client, request.app_id, request.depot_id, &request.manifest).await?;
    let size = client
        .download_depot_file(
            request.app_id,
            request.depot_id,
            manifest_id,
            &request.path,
            &dest,
            request.force,
        )
        .await?;
    Ok(DownloadedFile {
        manifest_id,
        path: dest,
        size,
    })
}

#[cfg(test)]
mod tests;
