//! The Steam Cloud overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Fetch the Steam Cloud file list for `app_id` (blocking) and open the
    /// overlay. On failure the overlay still opens, showing the error.
    pub fn open_cloud(&mut self, app_id: i32) {
        self.show_cloud = true;
        self.refresh_cloud(app_id);
    }

    /// Re-fetch the cloud file list for `app_id`, updating the status line.
    pub fn refresh_cloud(&mut self, app_id: i32) {
        match steam::cloud_list(app_id) {
            Ok(files) => {
                self.cloud_files = files;
                self.cloud_status.clear();
            }
            Err(err) => {
                self.cloud_files.clear();
                self.cloud_status = format!("Failed: {}", err);
            }
        }
    }

    /// Sync the game's Steam Cloud saves in `direction` (blocking), then re-fetch
    /// the list. `Both` syncs down then up; `Down`/`Up` restrict the transfer.
    pub fn sync_cloud(&mut self, app_id: i32, direction: steam::CloudDirection) {
        self.cloud_status = "syncing...".to_string();
        if let Err(err) = steam::cloud_sync(app_id, direction) {
            self.cloud_status = format!("Failed: {}", err);
            return;
        }
        self.refresh_cloud(app_id);
        if self.cloud_status.is_empty() {
            self.cloud_status = match direction {
                steam::CloudDirection::Both => "synced",
                steam::CloudDirection::Down => "downloaded",
                steam::CloudDirection::Up => "uploaded",
            }
            .to_string();
        }
    }

    /// Close the Steam Cloud overlay and drop its state.
    pub fn close_cloud(&mut self) {
        self.show_cloud = false;
        self.cloud_files.clear();
        self.cloud_status.clear();
    }
}
