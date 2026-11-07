//! Cross-process install locks.
//!
//! The in-process registry ([`super::active_installs`]) only sees this
//! process: an install `gaki-cli` runs (in its session daemon) is invisible
//! to `gaki-tui` and the other way round. So every running operation also
//! holds an OS file lock on `$XDG_STATE_HOME/gaki/installs/<app>.lock`.
//! The OS drops it when the process exits, crashes included, so a stale lock
//! can't outlive its owner. The files themselves stay; only the lock counts.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;

/// Where the lock files live.
fn lock_dir() -> Result<PathBuf> {
    #[cfg(test)]
    {
        Ok(std::env::temp_dir().join(format!("gaki-test-install-locks-{}", std::process::id())))
    }
    #[cfg(not(test))]
    {
        Ok(crate::core::config::state_dir()?.join("installs"))
    }
}

fn open(app_id: u32) -> Result<File> {
    let dir = lock_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("failed creating {}", dir.display()))?;
    let path = dir.join(format!("{app_id}.lock"));
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed opening {}", path.display()))
}

/// A held install lock; dropping it releases the lock.
#[derive(Debug)]
pub(crate) struct InstallLock {
    _file: File,
}

/// Take `app_id`'s lock, or `None` when another holder has it. On Linux
/// that includes another lock in this same process, so callers check the
/// in-process registry first.
pub(crate) fn try_acquire(app_id: u32) -> Result<Option<InstallLock>> {
    let file = open(app_id)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(InstallLock { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => {
            Err(e).with_context(|| format!("failed locking the install of app {app_id}"))
        }
    }
}

/// Whether something holds `app_id`'s lock right now. An error reading the
/// lock counts as free, so a broken state directory never blocks a resume
/// on its own.
pub(crate) fn is_held(app_id: u32) -> bool {
    match try_acquire(app_id) {
        Ok(lock) => lock.is_none(),
        Err(e) => {
            tracing::warn!("install lock for app {app_id}: {e:#}");
            false
        }
    }
}

#[cfg(test)]
mod tests;
