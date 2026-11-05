//! The running-games overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the games gaki currently has running (blocking) and open the
    /// overlay. A fetch error simply opens an empty overlay ("No games
    /// running."); the selection resets to the first row.
    pub fn open_running(&mut self) {
        self.running = steam::running().unwrap_or_default();
        self.running_index = 0;
        self.show_running = true;
    }

    /// Close the running overlay and drop its contents.
    pub fn close_running(&mut self) {
        self.show_running = false;
        self.running.clear();
        self.running_index = 0;
    }

    /// The highlighted running game, if any.
    pub fn selected_running(&self) -> Option<&steam::RunningJson> {
        self.running.get(self.running_index)
    }

    pub fn running_next(&mut self) {
        if self.running.is_empty() {
            self.running_index = 0;
            return;
        }
        self.running_index = (self.running_index + 1) % self.running.len();
    }

    pub fn running_previous(&mut self) {
        if self.running.is_empty() {
            self.running_index = 0;
            return;
        }
        self.running_index = if self.running_index == 0 {
            self.running.len() - 1
        } else {
            self.running_index - 1
        };
    }

    /// Re-fetch the running games after a stop, keeping the selection in range.
    pub fn refresh_running(&mut self) {
        self.running = steam::running().unwrap_or_default();
        if self.running.is_empty() {
            self.running_index = 0;
        } else if self.running_index >= self.running.len() {
            self.running_index = self.running.len() - 1;
        }
    }
}
