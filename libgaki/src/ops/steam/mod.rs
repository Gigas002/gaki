//! Keeping the official Steam client out of the way of file edits.
//!
//! Steam rewrites appmanifests and `libraryfolders.vdf` on exit, so editing
//! them while it runs loses the change.

use crate::core::error::{ErrorKind, TypedError};
use anyhow::Result;

/// Make sure Steam is not running before an edit.
///
/// If it is running: with `restart_steam` it is stopped (calling `on_stop`
/// first, e.g. to show a message) and `true` is returned so the caller can
/// [`restart_after_edit`]; without it, the edit is refused.
pub fn stop_for_edit(restart_steam: bool, on_stop: impl FnOnce()) -> Result<bool> {
    if !crate::core::utils::is_steam_running() {
        return Ok(false);
    }
    if !restart_steam {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            "Steam is running; close it first, or allow gaki to stop and restart it",
        )
        .into());
    }
    on_stop();
    crate::core::utils::shutdown_steam(std::time::Duration::from_secs(30))?;
    Ok(true)
}

/// Stop Steam if `want` and it is running, calling `on_stop` first. Returns
/// whether it was stopped. Unlike [`stop_for_edit`], a running Steam is fine.
pub fn stop_if_running(want: bool, on_stop: impl FnOnce()) -> Result<bool> {
    if !(want && crate::core::utils::is_steam_running()) {
        return Ok(false);
    }
    on_stop();
    crate::core::utils::shutdown_steam(std::time::Duration::from_secs(30))?;
    Ok(true)
}

/// Start Steam again if an earlier stop returned `true`, calling `on_start` first.
pub fn restart_after_edit(stopped: bool, on_start: impl FnOnce()) -> Result<()> {
    if stopped {
        on_start();
        crate::core::utils::start_steam_detached()?;
    }
    Ok(())
}
