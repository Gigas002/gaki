//! The help overlay.

use super::Browser;

impl Browser {
    /// Scroll the help overlay down by one row (clamped to the last row).
    pub fn help_scroll_down(&mut self) {
        let max = crate::ui::help::row_count().saturating_sub(1);
        if self.help_scroll < max {
            self.help_scroll += 1;
        }
    }

    /// Scroll the help overlay up by one row (clamped).
    pub fn help_scroll_up(&mut self) {
        self.help_scroll = self.help_scroll.saturating_sub(1);
    }
}
