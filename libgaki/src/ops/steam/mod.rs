//! Keeping the official Steam client out of the way of file edits.
//!
//! Steam rewrites appmanifests and `libraryfolders.vdf` on exit, so editing
//! them while it runs loses the change.

use crate::core::error::{ErrorKind, TypedError};
use crate::steam_client::SteamClient;
use anyhow::Result;

/// Make sure Steam is not running before an edit.
///
/// If it is running: with `restart_steam` it is stopped (calling `on_stop`
/// first, e.g. to show a message) and `true` is returned so the caller can
/// [`restart_after_edit`]; without it, the edit is refused.
pub fn stop_for_edit(restart_steam: bool, on_stop: impl FnOnce()) -> Result<bool> {
    if !SteamClient::steam_is_running() {
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
    SteamClient::shutdown_steam()?;
    Ok(true)
}

/// Stop Steam if `want` and it is running, calling `on_stop` first. Returns
/// whether it was stopped. Unlike [`stop_for_edit`], a running Steam is fine.
pub fn stop_if_running(want: bool, on_stop: impl FnOnce()) -> Result<bool> {
    if !(want && SteamClient::steam_is_running()) {
        return Ok(false);
    }
    on_stop();
    SteamClient::shutdown_steam()?;
    Ok(true)
}

/// Start Steam again if an earlier stop returned `true`, calling `on_start` first.
pub fn restart_after_edit(stopped: bool, on_start: impl FnOnce()) -> Result<()> {
    if stopped {
        on_start();
        SteamClient::start_steam()?;
    }
    Ok(())
}
