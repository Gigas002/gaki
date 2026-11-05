//! The inventory overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the inventory for `app_id` (blocking) and open the overlay. A fetch
    /// error simply opens an empty overlay ("No inventory items.").
    pub fn open_inventory(&mut self, app_id: i32) {
        self.inventory = steam::inventory(app_id).unwrap_or_default();
        self.inv_scroll = 0;
        self.show_inventory = true;
    }

    /// Close the inventory overlay and drop its data.
    pub fn close_inventory(&mut self) {
        self.show_inventory = false;
        self.inventory = Vec::new();
        self.inv_scroll = 0;
    }

    /// Scroll the inventory overlay down by one row (clamped).
    pub fn inv_scroll_down(&mut self) {
        let max = self.inventory.len().saturating_sub(1);
        if self.inv_scroll < max {
            self.inv_scroll += 1;
        }
    }

    /// Scroll the inventory overlay up by one row (clamped).
    pub fn inv_scroll_up(&mut self) {
        self.inv_scroll = self.inv_scroll.saturating_sub(1);
    }
}
