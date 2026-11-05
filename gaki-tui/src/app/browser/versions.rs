//! The versions and pinning overlay.

use std::sync::Arc;
use std::thread;

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the game's per-depot current manifests and pin/install state and
    /// open the overlay. A fetch error simply opens an empty overlay.
    pub fn open_versions(&mut self, app_id: i32) {
        self.versions_manifests = steam::manifests(app_id, None).unwrap_or_default();
        self.versions_available = steam::available(app_id).ok();
        self.versions_index = 0;
        self.versions_app_id = app_id;
        self.versions_status.clear();
        self.versions_input = None;
        self.show_versions = true;
    }

    /// Close the versions overlay and drop its contents.
    pub fn close_versions(&mut self) {
        self.show_versions = false;
        self.versions_manifests.clear();
        self.versions_available = None;
        self.versions_index = 0;
        self.versions_input = None;
    }

    pub fn versions_app_id(&self) -> i32 {
        self.versions_app_id
    }

    pub fn selected_manifest(&self) -> Option<&steam::DepotManifestInfo> {
        self.versions_manifests.get(self.versions_index)
    }

    pub fn versions_next(&mut self) {
        if self.versions_manifests.is_empty() {
            self.versions_index = 0;
            return;
        }
        self.versions_index = (self.versions_index + 1) % self.versions_manifests.len();
    }

    pub fn versions_previous(&mut self) {
        if self.versions_manifests.is_empty() {
            self.versions_index = 0;
            return;
        }
        self.versions_index = if self.versions_index == 0 {
            self.versions_manifests.len() - 1
        } else {
            self.versions_index - 1
        };
    }

    fn refresh_available(&mut self) {
        self.versions_available = steam::available(self.versions_app_id).ok();
    }

    /// Whether the game is currently pinned (so its updates are held).
    pub fn is_pinned(&self) -> bool {
        self.versions_available
            .as_ref()
            .map(|a| a.pinned)
            .unwrap_or(false)
    }

    /// Pin the game at its currently-installed depot versions.
    pub fn pin_current(&mut self) {
        self.versions_status = match steam::pin(self.versions_app_id) {
            Ok(p) => format!(
                "Pinned {} depot(s) at the installed version.",
                p.manifests.len()
            ),
            Err(e) => format!("Pin failed: {e}"),
        };
        self.refresh_available();
    }

    /// Clear the game's pin so it updates normally again.
    pub fn unpin_current(&mut self) {
        self.versions_status = match steam::unpin(self.versions_app_id) {
            Ok(true) => "Unpinned — updates re-enabled.".to_string(),
            Ok(false) => "Was not pinned.".to_string(),
            Err(e) => format!("Unpin failed: {e}"),
        };
        self.refresh_available();
    }

    /// Start prompting for a manifest id to downgrade the highlighted depot to.
    pub fn begin_downgrade_input(&mut self) {
        if self.selected_manifest().is_some() {
            self.versions_input = Some(String::new());
        }
    }

    pub fn versions_input_push(&mut self, c: char) {
        if c.is_ascii_digit()
            && let Some(s) = self.versions_input.as_mut()
        {
            s.push(c);
        }
    }

    pub fn versions_input_pop(&mut self) {
        if let Some(s) = self.versions_input.as_mut() {
            s.pop();
        }
    }

    pub fn cancel_downgrade_input(&mut self) {
        self.versions_input = None;
    }

    /// Kick off a downgrade of the highlighted depot to the typed manifest id on
    /// a worker thread. Progress streams into `versions_busy` (shown in the
    /// overlay); the download runs off the UI thread so the TUI stays responsive.
    pub fn start_downgrade(&mut self) {
        let Some(depot) = self.selected_manifest().map(|m| m.depot_id) else {
            return;
        };
        let Some(text) = self.versions_input.take() else {
            return;
        };
        let Ok(manifest) = text.trim().parse::<u64>() else {
            self.versions_status = "Invalid manifest id.".to_string();
            return;
        };
        let app_id = self.versions_app_id;
        let busy = Arc::clone(&self.versions_busy);
        self.versions_status = format!("Downgrading depot {depot} → manifest {manifest}…");
        thread::spawn(move || {
            steam::downgrade(app_id, &[(depot, manifest)], None, false, false, busy);
        });
    }

    /// The current downgrade worker status, if one is in flight (or just finished).
    pub fn downgrade_busy_text(&self) -> Option<String> {
        self.versions_busy
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.state.clone())
    }
}
