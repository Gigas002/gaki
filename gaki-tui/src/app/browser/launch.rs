//! The launch-options overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the launch options for `app_id` (blocking) and open the overlay. A
    /// fetch error simply opens an empty overlay ("No launch options.").
    pub fn open_launch(&mut self, app_id: i32) {
        self.launch_options = steam::launch_options(app_id).unwrap_or_default();
        self.launch_scroll = 0;
        self.show_launch = true;
    }

    /// Close the launch-options overlay and drop its data.
    pub fn close_launch(&mut self) {
        self.show_launch = false;
        self.launch_options = Vec::new();
        self.launch_scroll = 0;
    }

    /// Scroll the launch-options overlay down by one row (clamped).
    pub fn launch_scroll_down(&mut self) {
        let max = self.launch_options.len().saturating_sub(1);
        if self.launch_scroll < max {
            self.launch_scroll += 1;
        }
    }

    /// Scroll the launch-options overlay up by one row (clamped).
    pub fn launch_scroll_up(&mut self) {
        self.launch_scroll = self.launch_scroll.saturating_sub(1);
    }
}
