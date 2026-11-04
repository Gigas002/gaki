//! Bookkeeping for in-flight install/update/verify operations.

use crate::core::models::DownloadState;
use anyhow::{Result, bail};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// Shared progress state of one running operation.
pub type SharedDownloadState = Arc<RwLock<DownloadState>>;

/// In-flight operations in this process, by app id, so a later request (e.g.
/// "stop" or "list" arriving over another daemon connection, or another TUI
/// view) can reach a running download's shared state.
static ACTIVE_INSTALLS: OnceLock<Mutex<HashMap<u32, SharedDownloadState>>> = OnceLock::new();

/// The process-wide registry of in-flight operations.
pub fn active_installs() -> &'static Mutex<HashMap<u32, SharedDownloadState>> {
    ACTIVE_INSTALLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// RAII registration of a running operation: added on construction, removed on
/// drop (success, error, `?` or abort alike). Dropping it also raises the
/// operation's abort flag.
pub struct InstallGuard {
    app_id: u32,
    state: SharedDownloadState,
}

impl InstallGuard {
    /// Register a fresh shared [`DownloadState`] for `app_id`.
    pub fn begin(app_id: u32) -> Result<(Self, SharedDownloadState)> {
        let state = Arc::new(RwLock::new(DownloadState::default()));
        let guard = Self::register(app_id, Arc::clone(&state))?;
        Ok((guard, state))
    }

    /// Register `state` as `app_id`'s running operation. Fails if one is
    /// already running for that app.
    pub fn register(app_id: u32, state: SharedDownloadState) -> Result<Self> {
        let mut map = active_installs()
            .lock()
            .map_err(|_| anyhow::anyhow!("install registry lock poisoned"))?;
        if map.contains_key(&app_id) {
            bail!("an install/update/verify for app {app_id} is already running");
        }
        map.insert(app_id, Arc::clone(&state));
        Ok(Self { app_id, state })
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if let Ok(state) = self.state.read() {
            state.abort_signal.store(true, Ordering::SeqCst);
        }
        if let Ok(mut map) = active_installs().lock() {
            map.remove(&self.app_id);
        }
    }
}

#[cfg(test)]
mod tests;
