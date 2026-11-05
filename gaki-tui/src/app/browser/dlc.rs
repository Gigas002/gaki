//! The DLC overlay.

use super::Browser;
use crate::app::steam;
use crate::utils::error::TuiError;

impl Browser {
    /// Fetch the DLC for `app_id` and open the overlay (blocking); the
    /// selection resets to the first row.
    pub fn open_dlc(&mut self, app_id: i32) -> Result<(), TuiError> {
        self.dlc = steam::dlc(app_id)?;
        self.dlc_app_id = app_id;
        self.dlc_index = 0;
        self.show_dlc = true;
        Ok(())
    }

    /// Close the DLC overlay and drop its contents.
    pub fn close_dlc(&mut self) {
        self.show_dlc = false;
        self.dlc.clear();
        self.dlc_index = 0;
    }

    /// The highlighted DLC entry, if any.
    pub fn selected_dlc(&self) -> Option<&steam::Dlc> {
        self.dlc.get(self.dlc_index)
    }

    pub fn dlc_next(&mut self) {
        if self.dlc.is_empty() {
            self.dlc_index = 0;
            return;
        }
        self.dlc_index = (self.dlc_index + 1) % self.dlc.len();
    }

    pub fn dlc_previous(&mut self) {
        if self.dlc.is_empty() {
            self.dlc_index = 0;
            return;
        }
        self.dlc_index = if self.dlc_index == 0 {
            self.dlc.len() - 1
        } else {
            self.dlc_index - 1
        };
    }

    /// Re-fetch the open overlay's DLC after a toggle, keeping the selection in
    /// range.
    pub fn refresh_dlc(&mut self) -> Result<(), TuiError> {
        self.dlc = steam::dlc(self.dlc_app_id)?;
        if self.dlc.is_empty() {
            self.dlc_index = 0;
        } else if self.dlc_index >= self.dlc.len() {
            self.dlc_index = self.dlc.len() - 1;
        }
        Ok(())
    }
}
