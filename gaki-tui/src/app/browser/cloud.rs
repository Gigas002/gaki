//! The Steam Cloud overlay: the game's cloud files, syncing them, and
//! resolving saves that differ between Steam Cloud and this machine. The
//! list and the syncs load in the background.

use super::Browser;
use crate::app::browser::job::Load;
use crate::app::steam::{self, CloudConflict, CloudDirection, CloudResolve, CloudSyncResult};

/// The overlay's background work and conflicts.
#[derive(Default)]
pub struct CloudState {
    /// The game the overlay is showing.
    pub app_id: i32,
    /// The file list while it loads.
    pub list: Option<Load<Vec<steam::CloudFile>>>,
    /// A running sync, with how it was asked for.
    pub sync: Option<(CloudDirection, CloudResolve, Load<CloudSyncResult>)>,
    /// Saves the last sync left diverged.
    pub conflicts: Vec<CloudConflict>,
    /// A resolve waiting for its y/n (`KeepCloud` or `KeepLocal`).
    pub confirm: Option<CloudResolve>,
}

impl CloudState {
    pub fn is_busy(&self) -> bool {
        self.sync.is_some() || self.list.as_ref().is_some_and(Load::is_loading)
    }
}

impl Browser {
    /// Open the overlay for `app_id`; its file list loads in the background.
    pub fn open_cloud(&mut self, app_id: i32) {
        self.show_cloud = true;
        self.cloud_files.clear();
        self.cloud_status = "Loading…".to_string();
        self.cloud_state = CloudState {
            app_id,
            list: Some(Load::start(move || steam::cloud_list(app_id))),
            ..CloudState::default()
        };
    }

    /// Sync in `direction` (`s` both ways, `d` down, `u` up), leaving
    /// diverged saves alone. Refused while another sync runs.
    pub fn sync_cloud(&mut self, direction: CloudDirection) {
        self.start_cloud_sync(direction, CloudResolve::Detect);
    }

    fn start_cloud_sync(&mut self, direction: CloudDirection, resolve: CloudResolve) {
        if self.cloud_state.sync.is_some() {
            return;
        }
        let app_id = self.cloud_state.app_id;
        self.cloud_status = match resolve {
            CloudResolve::Detect => "Syncing…",
            CloudResolve::KeepCloud => "Keeping the cloud copies…",
            CloudResolve::KeepLocal => "Keeping the local copies…",
        }
        .to_string();
        let job = Load::start(move || steam::cloud_sync(app_id, direction, resolve));
        self.cloud_state.sync = Some((direction, resolve, job));
    }

    /// Ask to resolve every conflict one way (`c` keep cloud, `l` keep
    /// local). Only offered when the last sync left conflicts.
    pub fn begin_cloud_resolve(&mut self, resolve: CloudResolve) {
        if self.cloud_state.conflicts.is_empty() || self.cloud_state.sync.is_some() {
            return;
        }
        self.cloud_state.confirm = Some(resolve);
    }

    /// Answer the resolve confirm: `yes` syncs both ways with that side
    /// winning every conflict.
    pub fn answer_cloud_resolve(&mut self, yes: bool) {
        let Some(resolve) = self.cloud_state.confirm.take() else {
            return;
        };
        if yes {
            self.start_cloud_sync(CloudDirection::Both, resolve);
        }
    }

    /// Adopt a finished list load or sync. Called once per tick.
    pub fn poll_cloud(&mut self) {
        let state = &mut self.cloud_state;
        if let Some(list) = &mut state.list
            && list.poll()
        {
            match state.list.take() {
                Some(Load::Ready(files)) => {
                    self.cloud_files = files;
                    self.cloud_status.clear();
                }
                Some(Load::Failed(e)) => {
                    self.cloud_files.clear();
                    self.cloud_status = format!("Failed: {e}");
                }
                _ => {}
            }
        }
        let finished = match &mut self.cloud_state.sync {
            Some((_, _, job)) => job.poll(),
            None => false,
        };
        if !finished {
            return;
        }
        let Some((direction, resolve, job)) = self.cloud_state.sync.take() else {
            return;
        };
        match job {
            Load::Ready(result) => {
                self.cloud_files = result.files;
                self.cloud_state.conflicts = result.conflicts;
                self.cloud_status = match result.problem {
                    Some(problem) => problem,
                    None => done_message(direction, resolve).to_string(),
                };
            }
            Load::Failed(e) => self.cloud_status = format!("Failed: {e}"),
            Load::Loading(_) => {}
        }
    }

    /// Close the overlay. A running sync finishes on its own; its result is
    /// dropped.
    pub fn close_cloud(&mut self) {
        self.show_cloud = false;
        self.cloud_files.clear();
        self.cloud_status.clear();
        self.cloud_state = CloudState::default();
    }
}

fn done_message(direction: CloudDirection, resolve: CloudResolve) -> &'static str {
    match (resolve, direction) {
        (CloudResolve::KeepCloud, _) => "Kept the cloud copies.",
        (CloudResolve::KeepLocal, _) => "Kept the local copies.",
        (_, CloudDirection::Both) => "Synced.",
        (_, CloudDirection::Down) => "Downloaded.",
        (_, CloudDirection::Up) => "Uploaded.",
    }
}
