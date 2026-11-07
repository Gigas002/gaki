//! The move, relink and import prompts for an install on disk, and the
//! background job that runs them and uninstall.
//!
//! Each of these can take as long as copying or deleting a whole game, so it
//! runs on a thread and the UI keeps drawing; one runs at a time.

use super::Browser;
use super::job::{self, Slot};
use crate::app::steam;
use crate::utils::error::TuiError;

/// What a [`DiskJob`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskOp {
    Move,
    Relink,
    Import,
    Uninstall,
}

/// A move, relink, import or uninstall running in the background.
pub struct DiskJob {
    op: DiskOp,
    app_id: i32,
    slot: Slot<()>,
}

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

    /// Relocate the game to the typed library folder, in the background;
    /// [`Browser::poll_disk_job`] reports the outcome in `move_status`.
    pub fn do_move(&mut self) {
        let (app_id, path) = (self.move_app_id, self.move_path.clone());
        if self.start_disk_job(DiskOp::Move, app_id, move || {
            steam::move_game(app_id, &path)
        }) {
            self.move_status = "moving...".to_string();
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

    /// Relink the game to the typed library folder, in the background;
    /// [`Browser::poll_disk_job`] reports the outcome in `relink_status`.
    pub fn do_relink(&mut self) {
        let (app_id, path) = (self.relink_app_id, self.relink_path.clone());
        if self.start_disk_job(DiskOp::Relink, app_id, move || steam::relink(app_id, &path)) {
            self.relink_status = "relinking...".to_string();
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

    /// Register the on-disk install at the typed library folder, in the
    /// background; [`Browser::poll_disk_job`] reports the outcome in
    /// `import_status`.
    pub fn do_import(&mut self) {
        let (app_id, path) = (self.import_app_id, self.import_path.clone());
        if self.start_disk_job(DiskOp::Import, app_id, move || {
            steam::import_game(app_id, &path)
        }) {
            self.import_status = "importing...".to_string();
        }
    }

    /// Start `work` for `app_id` in the background, unless another disk job
    /// is running (then say so and return `false`).
    pub(super) fn start_disk_job(
        &mut self,
        op: DiskOp,
        app_id: i32,
        work: impl FnOnce() -> Result<(), TuiError> + Send + 'static,
    ) -> bool {
        if self.disk_job.is_some() {
            self.notice = Some("Wait for the current move, import or uninstall to finish.".into());
            return false;
        }
        self.disk_job = Some(DiskJob {
            op,
            app_id,
            slot: job::spawn(work),
        });
        true
    }

    /// Uninstall `app_id` in the background.
    pub fn start_uninstall(&mut self, app_id: i32) {
        self.start_disk_job(DiskOp::Uninstall, app_id, move || steam::uninstall(app_id));
    }

    /// Whether a disk job is running.
    pub fn disk_job_running(&self) -> bool {
        self.disk_job.is_some()
    }

    /// Adopt a finished disk job: show the outcome in its prompt (or the
    /// status bar, once the prompt is closed) and refresh that game's install
    /// state. Cheap while the job runs; call on every tick.
    pub fn poll_disk_job(&mut self) {
        let Some(running) = &self.disk_job else {
            return;
        };
        let Some(result) = job::take(&running.slot) else {
            return;
        };
        let (op, app_id) = (running.op, running.app_id);
        self.disk_job = None;
        self.finish_disk_job(op, app_id, result);
        self.request_game_refresh(app_id);
    }

    /// Show a finished disk job's outcome in its prompt, or in the status bar
    /// once the prompt is closed (a successful uninstall needs no message: the
    /// game's badge changes).
    fn finish_disk_job(&mut self, op: DiskOp, app_id: i32, result: Result<(), String>) {
        let line = match &result {
            Ok(()) => "done".to_string(),
            Err(e) => format!("Failed: {e}"),
        };
        let prompt = match op {
            DiskOp::Move if self.show_move && self.move_app_id == app_id => {
                Some(&mut self.move_status)
            }
            DiskOp::Relink if self.show_relink && self.relink_app_id == app_id => {
                Some(&mut self.relink_status)
            }
            DiskOp::Import if self.show_import && self.import_app_id == app_id => {
                Some(&mut self.import_status)
            }
            _ => None,
        };
        match (prompt, result) {
            (Some(status), _) => *status = line,
            (None, Ok(())) if op == DiskOp::Uninstall => {}
            (None, Ok(())) => self.notice = Some(format!("{op:?} finished.")),
            (None, Err(e)) => self.notice = Some(format!("{op:?} failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests;
