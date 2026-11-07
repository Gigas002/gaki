//! Depot browser: a depot manifest's entries, and writing one file of it.
//!
//! The network side (content servers, request code, depot key, manifest) is
//! [`SteamClient::depot_manifest_files`] and
//! [`SteamClient::download_depot_file`]; this module turns a manifest into
//! [`ManifestFileEntry`] rows, picks the file to download and writes it.
//!
//! [`SteamClient::depot_manifest_files`]: crate::steam_client::SteamClient::depot_manifest_files
//! [`SteamClient::download_depot_file`]: crate::steam_client::SteamClient::download_depot_file

use anyhow::{Context, Result, anyhow, bail};
use gaki_steam_cdn::{DepotManifest, FLAG_DIRECTORY, FLAG_EXECUTABLE, FLAG_SYMLINK, ManifestFile};
use sha1::Digest;
use std::io::Read;
use std::path::{Path, PathBuf};

/// What a manifest entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

/// One entry of a depot manifest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestFileEntry {
    /// Path inside the depot, `/`-separated.
    pub path: String,
    pub kind: EntryKind,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// SHA-1 of the content, hex (empty for directories).
    pub sha1: String,
    pub chunks: usize,
    pub executable: bool,
    /// A symlink's target, when the manifest carries it as a field (some
    /// store it as chunk content instead).
    pub link_target: Option<String>,
}

/// An entry's kind from its manifest flags and link target.
pub(crate) fn entry_kind(flags: u32, link_target: &str) -> EntryKind {
    if flags & FLAG_DIRECTORY != 0 {
        EntryKind::Directory
    } else if !link_target.is_empty() || flags & FLAG_SYMLINK != 0 {
        EntryKind::Symlink
    } else {
        EntryKind::File
    }
}

fn entry(file: &ManifestFile) -> ManifestFileEntry {
    let link_target = file.linktarget();
    let kind = entry_kind(file.flags(), &link_target);
    ManifestFileEntry {
        path: file.full_path(),
        kind,
        size: file.size(),
        sha1: if kind == EntryKind::Directory {
            String::new()
        } else {
            hex::encode(file.sha_content())
        },
        chunks: file.chunks().len(),
        executable: file.flags() & FLAG_EXECUTABLE != 0,
        link_target: (!link_target.is_empty()).then(|| link_target.replace('\\', "/")),
    }
}

/// Every entry of `manifest`, sorted by path. Entries with an empty path
/// (the depot root) are left out.
pub(crate) fn entries(manifest: &DepotManifest) -> Vec<ManifestFileEntry> {
    let mut out: Vec<_> = manifest
        .files()
        .iter()
        .map(entry)
        .filter(|e| !e.path.is_empty())
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// A requested depot path in manifest form: `/`-separated, no leading `/`
/// or `./`.
pub(crate) fn normalize_request(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let mut path = path.as_str();
    loop {
        if let Some(rest) = path.strip_prefix("./") {
            path = rest;
        } else if let Some(rest) = path.strip_prefix('/') {
            path = rest;
        } else {
            break;
        }
    }
    path.to_string()
}

/// The regular file at `path` in `manifest`. Directories and symlinks are an
/// error: only file content can be downloaded.
pub(crate) fn find_file<'m>(manifest: &'m DepotManifest, path: &str) -> Result<&'m ManifestFile> {
    let wanted = normalize_request(path);
    let file = manifest
        .files()
        .iter()
        .find(|f| f.full_path() == wanted)
        .ok_or_else(|| anyhow!("{wanted} is not in this manifest (paths are case-sensitive)"))?;
    match entry_kind(file.flags(), &file.linktarget()) {
        EntryKind::File => Ok(file),
        EntryKind::Directory => bail!("{wanted} is a directory; download its files one by one"),
        EntryKind::Symlink => bail!("{wanted} is a symlink, not a file"),
    }
}

/// Where a depot file lands: `out_dir/<file name>`. The depot path goes
/// through the same check as an install (no `..`, not absolute, not empty),
/// so a forged manifest or a typo can't name a path outside `out_dir`.
pub fn download_target(out_dir: &Path, depot_path: &str) -> Result<PathBuf> {
    let relative =
        gaki_steam_cdn::entry_path(&normalize_request(depot_path)).map_err(|e| anyhow!("{e}"))?;
    let name = relative
        .file_name()
        .ok_or_else(|| anyhow!("{depot_path:?} has no file name"))?;
    Ok(out_dir.join(name))
}

/// The partial file a download writes before it is renamed into place.
fn part_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.gaki-part"))
}

/// SHA-1 of the file at `path`, hex.
fn sha1_file(path: &Path) -> Result<String> {
    let mut reader =
        std::fs::File::open(path).with_context(|| format!("failed opening {}", path.display()))?;
    let mut hasher = sha1::Sha1::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Whether `dest` may be written: refuse an existing entry unless
/// `overwrite`.
pub(crate) fn check_dest(dest: &Path, overwrite: bool) -> Result<()> {
    if !overwrite && dest.symlink_metadata().is_ok() {
        bail!(
            "{} already exists (use --force to overwrite)",
            dest.display()
        );
    }
    if dest.is_dir() {
        bail!("{} is a directory", dest.display());
    }
    Ok(())
}

/// Move the finished `part` to `dest`. Without `overwrite` a hard link
/// puts it there, which fails if anything has appeared at `dest` since
/// [`check_dest`]; with it, a rename replaces whatever is there.
pub(crate) fn place(part: &Path, dest: &Path, overwrite: bool) -> Result<()> {
    if overwrite {
        std::fs::rename(part, dest)
            .with_context(|| format!("failed moving the download to {}", dest.display()))
    } else {
        match std::fs::hard_link(part, dest) {
            Ok(()) => {
                let _ = std::fs::remove_file(part);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                bail!(
                    "{} already exists (use --force to overwrite)",
                    dest.display()
                )
            }
            // No hard links on this filesystem (FAT, some FUSE mounts):
            // check again and rename.
            Err(_) => {
                check_dest(dest, false)?;
                std::fs::rename(part, dest)
                    .with_context(|| format!("failed moving the download to {}", dest.display()))
            }
        }
    }
}

/// Download `file` to `dest`. The content goes to a fresh partial file next
/// to `dest`, is checked against the manifest's SHA-1, and only then renamed
/// over `dest` (which replaces a symlink there rather than writing through
/// it); without `overwrite` an existing `dest` is never replaced. Returns
/// the size written.
pub(crate) async fn write_file(
    file: &ManifestFile,
    depot_key: [u8; 32],
    dest: &Path,
    overwrite: bool,
) -> Result<u64> {
    check_dest(dest, overwrite)?;
    let part = part_path(dest);
    match std::fs::remove_file(&part) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("failed removing {}", part.display())),
    }
    // `create_new` never follows a link someone left at the partial path.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .with_context(|| format!("failed creating {}", part.display()))?;

    let written = async {
        file.download(depot_key, &part, false, None, None, None)
            .await
            .map_err(|e| anyhow!("failed to download {}: {e}", file.full_path()))?;
        let expected = hex::encode(file.sha_content());
        if !expected.is_empty() && file.size() > 0 {
            let actual = sha1_file(&part)?;
            if actual != expected {
                bail!(
                    "{} failed its checksum (expected SHA-1 {expected}, got {actual})",
                    file.full_path()
                );
            }
        }
        place(&part, dest, overwrite)?;
        Ok(file.size())
    }
    .await;
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written
}

#[cfg(test)]
mod tests;
