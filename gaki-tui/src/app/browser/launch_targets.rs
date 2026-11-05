//! The launch targets ("Play as…") overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the game's launch targets and open the
    /// overlay, highlighting the target plain `play` would pick.
    pub fn open_launch_targets(&mut self, app_id: i32) {
        self.launch_targets_status.clear();
        self.launch_targets = match steam::launch_targets(app_id) {
            Ok(targets) => targets,
            Err(e) => {
                self.launch_targets_status = format!("Failed: {e}");
                Vec::new()
            }
        };
        self.launch_targets_index = self
            .launch_targets
            .iter()
            .position(|t| t.default)
            .unwrap_or(0);
        self.show_launch_targets = true;
    }

    pub fn close_launch_targets(&mut self) {
        self.show_launch_targets = false;
        self.launch_targets.clear();
        self.launch_targets_index = 0;
        self.launch_targets_status.clear();
    }

    pub fn launch_targets_next(&mut self) {
        if !self.launch_targets.is_empty() {
            self.launch_targets_index = (self.launch_targets_index + 1) % self.launch_targets.len();
        }
    }

    pub fn launch_targets_previous(&mut self) {
        if !self.launch_targets.is_empty() {
            self.launch_targets_index = if self.launch_targets_index == 0 {
                self.launch_targets.len() - 1
            } else {
                self.launch_targets_index - 1
            };
        }
    }

    /// The `play` options for the highlighted target, or `None` (with a status
    /// explaining why) when it cannot be launched.
    pub fn selected_launch_target(&mut self) -> Option<steam::PlayOpts> {
        let target = self.launch_targets.get(self.launch_targets_index)?;
        if !target.available {
            let missing = target.missing.as_deref().unwrap_or("unknown");
            self.launch_targets_status = format!("Unavailable (missing: {missing}).");
            return None;
        }
        Some(target.play_opts())
    }
}
