//! The branches overlay.

use super::Browser;
use crate::interface::aurelia;
use crate::utils::error::STError;

impl Browser {
    /// Fetch the beta branches for `app_id` and open the overlay (blocking);
    /// the selection resets to the first row.
    pub fn open_branches(&mut self, app_id: i32) -> Result<(), STError> {
        self.branches = aurelia::branches(app_id)?;
        self.branch_app_id = app_id;
        self.branch_index = 0;
        self.show_branches = true;
        Ok(())
    }

    /// Close the branches overlay and drop its contents.
    pub fn close_branches(&mut self) {
        self.show_branches = false;
        self.branches.clear();
        self.branch_index = 0;
    }

    /// The app id the branches overlay was opened for.
    pub fn branch_app_id(&self) -> i32 {
        self.branch_app_id
    }

    /// The highlighted branch entry, if any.
    pub fn selected_branch(&self) -> Option<&aurelia::BranchJson> {
        self.branches.get(self.branch_index)
    }

    pub fn branch_next(&mut self) {
        if self.branches.is_empty() {
            self.branch_index = 0;
            return;
        }
        self.branch_index = (self.branch_index + 1) % self.branches.len();
    }

    pub fn branch_previous(&mut self) {
        if self.branches.is_empty() {
            self.branch_index = 0;
            return;
        }
        self.branch_index = if self.branch_index == 0 {
            self.branches.len() - 1
        } else {
            self.branch_index - 1
        };
    }
}
