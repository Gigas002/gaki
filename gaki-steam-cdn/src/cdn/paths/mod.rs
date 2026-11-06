// Part of gaki-steam-cdn (gaki's addition to the steam-cdn fork); see README.md.
//! Keeping a depot inside its install directory.
//!
//! A manifest lists every entry's path and every symlink's target, and nothing
//! else stops a forged manifest (from a compromised server, or over plain
//! HTTP) from naming `/home/you/.bashrc` or `../../.config`. These checks run
//! before anything is created, removed or written.

use std::path::{Component, Path, PathBuf};

use crate::Error;

/// Longest symlink target read from chunk data (Linux's `PATH_MAX`).
pub(crate) const MAX_LINK_TARGET: u64 = 4096;

/// The entry's path relative to the install directory, or an error if it is
/// empty, absolute, or climbs out with `..`.
pub(crate) fn entry_path(raw: &str) -> Result<PathBuf, Error> {
    let mut out = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::UnsafePath(format!(
                    "manifest entry {raw:?} leaves the install directory"
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(Error::UnsafePath(format!(
            "manifest entry {raw:?} is empty"
        )));
    }
    Ok(out)
}

/// Whether a symlink in `parent` (its real directory, relative to the install
/// directory, with no symlinks in it) pointing at `target` stays inside.
///
/// `..` may only lead the target: the kernel resolves each component before
/// the next, so in `dir/..` a `dir` that is itself a link would be followed
/// first, and no reading of the text alone can say where that ends up.
/// Absolute targets never stay inside.
pub(crate) fn link_stays_inside(parent: &Path, target: &str) -> bool {
    let mut depth = parent
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    let mut descended = false;
    for component in Path::new(target).components() {
        match component {
            Component::Normal(_) => descended = true,
            Component::CurDir => {}
            Component::ParentDir if !descended && depth > 0 => depth -= 1,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// Check that `path` resolves inside `root` (already canonical) through
/// whatever exists on disk: the nearest existing ancestor is canonicalized, so
/// a symlink already in the tree can't carry a write outside it. `path` itself
/// isn't followed; callers replace a symlink there instead of writing
/// through it.
pub(crate) fn ensure_inside(root: &Path, path: &Path) -> Result<(), Error> {
    let mut existing = path.parent();
    while let Some(dir) = existing {
        if dir.symlink_metadata().is_ok() {
            let resolved = dir.canonicalize().map_err(Error::Io)?;
            if resolved.starts_with(root) {
                return Ok(());
            }
            return Err(Error::UnsafePath(format!(
                "{} resolves outside the install directory",
                path.display()
            )));
        }
        existing = dir.parent();
    }
    Err(Error::UnsafePath(format!(
        "{} has no existing parent",
        path.display()
    )))
}

/// Remove a symlink at `path`, if there is one, so that writing a regular
/// file there creates it rather than following the link.
pub(crate) fn unlink_symlink(path: &Path) -> Result<(), Error> {
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::remove_file(path).map_err(Error::Io),
        _ => Ok(()),
    }
}

/// What a manifest entry is.
pub(crate) enum Entry {
    Dir,
    /// A symlink, with its target (`/`-separated).
    Link(String),
    File,
}

/// Create one manifest entry under `root` (canonical): directories and
/// symlinks are made here, and a regular file's path is returned for the
/// download to write. Entries that would land outside `root` are an error;
/// symlinks that would point outside are skipped with a warning, since one
/// odd link shouldn't fail an install.
pub(crate) fn place(root: &Path, raw: &str, entry: Entry) -> Result<Option<PathBuf>, Error> {
    let full_path = root.join(entry_path(raw)?);
    ensure_inside(root, &full_path)?;

    match entry {
        Entry::Dir => {
            unlink_symlink(&full_path)?;
            std::fs::create_dir_all(&full_path).map_err(Error::Io)?;
            Ok(None)
        }
        Entry::Link(target) => {
            create_parent(&full_path)?;
            if target.is_empty() {
                return Err(Error::Unexpected(format!(
                    "symlink entry {raw} has no target"
                )));
            }
            // Judged from the link's real directory: a parent reached through
            // another link is wherever that link points.
            let parent = real_parent(root, &full_path)?;
            if !link_stays_inside(&parent, &target) {
                tracing::warn!(
                    "skipping symlink {raw} -> {target}: it points outside the install directory"
                );
                return Ok(None);
            }
            let points_there =
                std::fs::read_link(&full_path).is_ok_and(|existing| existing == Path::new(&target));
            if !points_there {
                let _ = std::fs::remove_file(&full_path);
                std::os::unix::fs::symlink(&target, &full_path).map_err(Error::Io)?;
            }
            Ok(None)
        }
        Entry::File => {
            create_parent(&full_path)?;
            // Written in place, never through a link.
            unlink_symlink(&full_path)?;
            Ok(Some(full_path))
        }
    }
}

fn create_parent(path: &Path) -> Result<(), Error> {
    match path.parent() {
        Some(parent) => std::fs::create_dir_all(parent).map_err(Error::Io),
        None => Ok(()),
    }
}

/// `path`'s parent, resolved on disk, relative to `root`.
fn real_parent(root: &Path, path: &Path) -> Result<PathBuf, Error> {
    let parent = path
        .parent()
        .unwrap_or(root)
        .canonicalize()
        .map_err(Error::Io)?;
    parent
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|_| {
            Error::UnsafePath(format!(
                "{} resolves outside the install directory",
                path.display()
            ))
        })
}

#[cfg(test)]
mod tests;
