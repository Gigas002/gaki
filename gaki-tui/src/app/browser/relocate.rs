//! The move, relink and import prompts for an install on disk.

use super::Browser;
use crate::app::steam;
use crate::utils::error::TuiError;

impl Browser {
    /// Open the move prompt for `app_id`, clearing any previous path/status.
    pub fn open_move(&mut self, app_id: i32) {
        self.move_app_id = app_id;
        self.move_path.clear();
        self.move_status.clear();
        self.show_move = true;
    }

    /// Close the move prompt and drop its state.
    pub fn close_move(&mut self) {
        self.show_move = false;
        self.move_path.clear();
        self.move_status.clear();
        self.move_app_id = 0;
    }

    /// Append a typed character to the destination path.
    pub fn move_push(&mut self, c: char) {
        self.move_path.push(c);
    }

    /// Remove the last character from the destination path.
    pub fn move_pop(&mut self) {
        self.move_path.pop();
    }

    /// Relocate the game to the typed library folder (blocking). Updates `move_status` to reflect progress/outcome and returns the
    /// backend result.
    pub fn do_move(&mut self) -> Result<(), TuiError> {
        self.move_status = "moving...".to_string();
        match steam::move_game(self.move_app_id, &self.move_path) {
            Ok(()) => {
                self.move_status = "done".to_string();
                Ok(())
            }
            Err(err) => {
                self.move_status = format!("Failed: {}", err);
                Err(err)
            }
        }
    }

    // --- Relink (relink install) prompt ---

    /// Open the relink prompt for `app_id`, clearing any previous path/status.
    pub fn open_relink(&mut self, app_id: i32) {
        self.relink_app_id = app_id;
        self.relink_path.clear();
        self.relink_status.clear();
        self.show_relink = true;
    }

    /// Close the relink prompt and drop its state.
    pub fn close_relink(&mut self) {
        self.show_relink = false;
        self.relink_path.clear();
        self.relink_status.clear();
        self.relink_app_id = 0;
    }

    /// Append a typed character to the destination path.
    pub fn relink_push(&mut self, c: char) {
        self.relink_path.push(c);
    }

    /// Remove the last character from the destination path.
    pub fn relink_pop(&mut self) {
        self.relink_path.pop();
    }

    /// Relink the game to the typed library folder (blocking). Updates `relink_status` to reflect progress/outcome and returns
    /// the backend result.
    pub fn do_relink(&mut self) -> Result<(), TuiError> {
        self.relink_status = "relinking...".to_string();
        match steam::relink(self.relink_app_id, &self.relink_path) {
            Ok(()) => {
                self.relink_status = "done".to_string();
                Ok(())
            }
            Err(err) => {
                self.relink_status = format!("Failed: {}", err);
                Err(err)
            }
        }
    }

    // --- Import (register existing install) prompt ---

    /// Open the import prompt for `app_id`, clearing any previous path/status.
    pub fn open_import(&mut self, app_id: i32) {
        self.import_app_id = app_id;
        self.import_path.clear();
        self.import_status.clear();
        self.show_import = true;
    }

    /// Close the import prompt and drop its state.
    pub fn close_import(&mut self) {
        self.show_import = false;
        self.import_path.clear();
        self.import_status.clear();
        self.import_app_id = 0;
    }

    /// Append a typed character to the library path.
    pub fn import_push(&mut self, c: char) {
        self.import_path.push(c);
    }

    /// Remove the last character from the library path.
    pub fn import_pop(&mut self) {
        self.import_path.pop();
    }

    /// Register the on-disk install at the typed library folder (blocking).
    /// Updates `import_status` to reflect progress/outcome and
    /// returns the backend result.
    pub fn do_import(&mut self) -> Result<(), TuiError> {
        self.import_status = "importing...".to_string();
        match steam::import_game(self.import_app_id, &self.import_path) {
            Ok(()) => {
                self.import_status = "done".to_string();
                Ok(())
            }
            Err(err) => {
                self.import_status = format!("Failed: {}", err);
                Err(err)
            }
        }
    }
}
