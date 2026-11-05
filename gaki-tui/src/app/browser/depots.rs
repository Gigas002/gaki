//! The depots overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the depots for `app_id` (blocking) and open the overlay. A fetch
    /// error simply opens an empty overlay ("No depots.").
    pub fn open_depots(&mut self, app_id: i32) {
        self.depots = steam::depots(app_id).unwrap_or_default();
        self.depots_scroll = 0;
        self.show_depots = true;
    }

    /// Close the depots overlay and drop its contents.
    pub fn close_depots(&mut self) {
        self.show_depots = false;
        self.depots.clear();
        self.depots_scroll = 0;
    }

    /// Scroll the depots overlay down by one row (clamped).
    pub fn depots_scroll_down(&mut self) {
        let max = self.depots.len().saturating_sub(1);
        if self.depots_scroll < max {
            self.depots_scroll += 1;
        }
    }

    /// Scroll the depots overlay up by one row (clamped).
    pub fn depots_scroll_up(&mut self) {
        self.depots_scroll = self.depots_scroll.saturating_sub(1);
    }
}
