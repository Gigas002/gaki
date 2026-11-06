//! Installs from the library: the location picker, start/pause/cancel, and
//! adopting finished installs and single-game refreshes.

use std::sync::{Arc, Mutex};
use std::thread;

use super::Browser;
use crate::app::game::Game;
use crate::app::steam;

/// Where a game's install currently sits, from the UI's point of view. Drives
/// whether the pause / resume / stop keys do anything for the selected game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    /// No UI-tracked install for this game.
    Idle,
    /// A download is in progress (pausable / stoppable).
    Active,
    /// The download was paused (resumable / stoppable).
    Paused,
}

impl Browser {
    /// Open the install-location picker for `app_id`, fetching the available
    /// Steam library folders (blocking). Returns `false` without opening if no
    /// libraries could be enumerated, so the caller can fall back to a default
    /// install.
    pub fn open_install_picker(&mut self, app_id: i32) -> bool {
        let libraries = steam::libraries().unwrap_or_default();
        if libraries.is_empty() {
            return false;
        }
        self.install_libraries = libraries;
        // The on-disk estimate, for showing the size and gauging fit. Treat 0
        // (unavailable) as unknown.
        self.install_estimate = steam::install_estimate(app_id).ok().filter(|&b| b > 0);
        self.install_picker_index = 0;
        self.show_install_picker = true;
        true
    }

    /// Close the install-location picker.
    pub fn close_install_picker(&mut self) {
        self.show_install_picker = false;
        self.install_libraries.clear();
        self.install_estimate = None;
        self.install_picker_index = 0;
    }

    /// Whether the highlighted library has room for the estimated install. True
    /// when either the estimate or the library's free space is unknown — don't
    /// block on missing data; the install itself re-checks.
    pub fn selected_library_fits(&self) -> bool {
        match self.install_libraries.get(self.install_picker_index) {
            Some(lib) => match (self.install_estimate, lib.free_bytes) {
                (Some(est), Some(free)) => free >= est,
                _ => true,
            },
            None => false,
        }
    }

    /// Move the install-picker highlight down (clamped).
    pub fn install_picker_next(&mut self) {
        let max = self.install_libraries.len().saturating_sub(1);
        if self.install_picker_index < max {
            self.install_picker_index += 1;
        }
    }

    /// Move the install-picker highlight up (clamped).
    pub fn install_picker_previous(&mut self) {
        self.install_picker_index = self.install_picker_index.saturating_sub(1);
    }

    /// The library folder path currently highlighted in the install picker.
    pub fn selected_install_library(&self) -> Option<String> {
        self.install_libraries
            .get(self.install_picker_index)
            .map(|lib| lib.path.clone())
    }

    /// Register a fresh install control for `app_id` (replacing any prior one),
    /// remember the chosen `library` (so a later resume targets the same one),
    /// and return a clone to hand to [`crate::app::client::Client::install`]. Called
    /// when the UI starts (or resumes) an install.
    pub fn begin_install(&mut self, app_id: i32, library: Option<String>) -> steam::InstallControl {
        let control = steam::InstallControl::new();
        self.install_controls.insert(app_id, control.clone());
        self.install_library_choice.insert(app_id, library);
        control
    }

    /// The library a tracked install of `app_id` was started in, for resuming a
    /// paused download into the same place.
    pub fn install_library_for(&self, app_id: i32) -> Option<String> {
        self.install_library_choice.get(&app_id).cloned().flatten()
    }

    /// How the selected game's UI-tracked install currently sits. Reads the live
    /// status cell, but only for games this session actually started installing.
    pub fn install_phase(&self, game: &Game) -> InstallPhase {
        if !self.install_controls.contains_key(&game.id) {
            return InstallPhase::Idle;
        }
        let state = game.get_status().map(|s| s.state).unwrap_or_default();
        if state.starts_with("paused") {
            InstallPhase::Paused
        } else if state.contains("downloading")
            || state.contains("processing")
            || state.contains("queued")
            || state.contains("verifying")
            || state.contains("moving")
        {
            InstallPhase::Active
        } else {
            InstallPhase::Idle
        }
    }

    /// Pause the in-flight install of `app_id`: flag its control so the worker
    /// finalises as "paused", then pause the download (it is left on disk, and
    /// its backlog entry stays paused, so [`Browser::begin_install`] resumes
    /// it). A no-op if no
    /// install is tracked for the game.
    pub fn pause_install(&mut self, app_id: i32) {
        if let Some(control) = self.install_controls.get(&app_id) {
            control.set(steam::InstallAction::Paused);
            let _ = steam::install_pause(app_id);
        }
    }

    /// Stop (cancel) the install of `app_id`: flag its control so the worker
    /// clears the status, then ask the backend to abort the download. The
    /// game is reset to not-installed immediately (badge + flag), and a
    /// background re-fetch confirms the on-disk state shortly after.
    pub fn stop_install(&mut self, app_id: i32) {
        if let Some(control) = self.install_controls.remove(&app_id) {
            self.install_library_choice.remove(&app_id);
            control.set(steam::InstallAction::Stopped);
            let _ = steam::install_stop(app_id);
            if let Some(game) = self.items.iter_mut().find(|g| g.id == app_id) {
                game.installed = false;
                if let Ok(mut s) = game.status_counter().lock() {
                    *s = None;
                }
            }
            self.request_game_refresh(app_id);
        }
    }

    /// Spawn an off-thread `list` fetch and drop the matching game's fresh entry
    /// into `game_refresh_slot` for the next poll to adopt. Used to
    /// reconcile one game's install state after an install completes/cancels
    /// without a full-screen reload.
    pub fn request_game_refresh(&self, app_id: i32) {
        let slot = Arc::clone(&self.game_refresh_slot);
        thread::spawn(move || {
            if let Ok(entries) = steam::fetch_library()
                && let Some(entry) = entries.into_iter().find(|e| e.app_id as i32 == app_id)
                && let Ok(mut guard) = slot.lock()
            {
                guard.push(entry);
            }
        });
    }

    /// Check the installed games for updates off-thread, unless a check is
    /// already running. [`Browser::poll_update_check`] adopts the result.
    pub fn request_update_check(&mut self) {
        if self.update_check.is_some() {
            return;
        }
        let slot: UpdateSlot = Arc::new(Mutex::new(None));
        self.update_check = Some(Arc::clone(&slot));
        thread::spawn(move || {
            let result = steam::fetch_updates().map_err(|e| e.to_string());
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(result);
            }
        });
    }

    /// Whether a background update check is running.
    pub fn checking_updates(&self) -> bool {
        self.update_check.is_some()
    }

    /// Adopt a finished update check: flag the installed games it found and
    /// clear the rest. A failed check leaves the flags alone and says so in
    /// the status bar. Cheap and non-blocking while the check runs.
    pub fn poll_update_check(&mut self) {
        let Some(slot) = &self.update_check else {
            return;
        };
        let finished = match slot.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => Some(Err("the update check crashed".to_string())),
        };
        let Some(result) = finished else {
            return;
        };
        self.update_check = None;
        match result {
            Ok(ids) => {
                apply_updates(&mut self.items, &ids);
                // The Updates tab may have shrunk under the selection.
                self.clamp_selection();
            }
            Err(e) => {
                tracing::warn!("update check failed: {e}");
                self.notice = Some(format!("Couldn't check for updates: {e}"));
            }
        }
    }

    /// Adopt any freshly-fetched single-game entries: patch the in-memory game's
    /// install flags and drop its transient install status so the badge derives
    /// from the refreshed state. Cheap and non-blocking when idle.
    pub fn poll_game_refresh(&mut self) {
        let updates: Vec<steam::LibraryGame> = match self.game_refresh_slot.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(_) => Vec::new(),
        };
        if updates.is_empty() {
            return;
        }
        for entry in updates {
            let id = entry.app_id as i32;
            if let Some(game) = self.items.iter_mut().find(|g| g.id == id) {
                game.installed = entry.is_installed;
                game.install_dir = entry.install_path.clone().unwrap_or_default();
                game.update_available = entry.update_available;
                if let Ok(mut s) = game.status_counter().lock() {
                    *s = None;
                }
            }
        }
        self.clamp_selection();
    }

    /// Detect tracked installs that have just finished and reconcile them. On
    /// success a targeted re-fetch updates the game's `installed` flag (so the
    /// listing no longer wrongly offers uninstall on a half-installed game); a
    /// failed install keeps its badge. Either way the finished control is
    /// dropped. Call once per event-loop iteration.
    pub fn poll_install_completions(&mut self) {
        let terminal: Vec<(i32, bool)> = self
            .install_controls
            .keys()
            .copied()
            .filter_map(|id| {
                let state = self
                    .items
                    .iter()
                    .find(|g| g.id == id)
                    .map(|g| g.get_status().map(|s| s.state).unwrap_or_default())
                    .unwrap_or_default();
                if state.starts_with("Installed!") || state == "done" {
                    Some((id, true))
                } else if state.starts_with("Failed") {
                    Some((id, false))
                } else {
                    None
                }
            })
            .collect();
        for (id, succeeded) in terminal {
            self.install_controls.remove(&id);
            self.install_library_choice.remove(&id);
            if succeeded {
                self.request_game_refresh(id);
            }
        }
    }
}

/// Where a background update check leaves its result: the app ids with an
/// update, or the error.
pub(crate) type UpdateSlot = Arc<Mutex<Option<Result<Vec<u32>, String>>>>;

/// Flag the installed games in `ids` as having an update and clear the rest.
pub(crate) fn apply_updates(items: &mut [Game], ids: &[u32]) {
    for game in items.iter_mut().filter(|g| g.installed) {
        game.update_available = ids.iter().any(|&id| id as i32 == game.id);
    }
}

#[cfg(test)]
mod tests;
