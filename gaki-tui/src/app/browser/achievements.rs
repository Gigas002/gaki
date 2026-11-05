//! The achievements overlay.

use super::Browser;
use crate::interface::aurelia;

impl Browser {
    /// Fetch the selected game's achievements (blocking) and open the overlay.
    /// A fetch error simply opens an empty overlay ("No achievements.").
    pub fn open_achievements(&mut self) {
        let Some(game) = self.selected() else {
            return;
        };
        self.achievements = aurelia::achievements(game.id).unwrap_or_default();
        self.ach_scroll = 0;
        self.show_achievements = true;
    }

    /// Close the achievements overlay and drop its data.
    pub fn close_achievements(&mut self) {
        self.show_achievements = false;
        self.achievements = Vec::new();
        self.ach_scroll = 0;
    }

    /// Scroll the achievements overlay down by one row (clamped).
    pub fn ach_scroll_down(&mut self) {
        let max = self.achievements.len().saturating_sub(1);
        if self.ach_scroll < max {
            self.ach_scroll += 1;
        }
    }

    /// Scroll the achievements overlay up by one row (clamped).
    pub fn ach_scroll_up(&mut self) {
        self.ach_scroll = self.ach_scroll.saturating_sub(1);
    }
}
