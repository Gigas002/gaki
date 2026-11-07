//! The depots overlay and, with the `depot` feature, its file view: a
//! depot's manifest entries, a filter, a branch switch and single-file
//! downloads. Every fetch runs on a thread; the tick adopts the results
//! ([`Browser::poll_depots`]), so the UI never waits on Steam.

use super::Browser;
pub use super::job::Load;
#[cfg(feature = "depot")]
use super::job::Slot;
use crate::app::steam;

/// The depots overlay's state.
#[derive(Default)]
pub struct DepotsView {
    /// The game the overlay was opened for.
    pub app_id: i32,
    pub list: Load<Vec<steam::Depot>>,
    /// Highlighted depot.
    pub selected: usize,
    /// The open file view, if any.
    #[cfg(feature = "depot")]
    pub files: Option<files::FilesView>,
    /// The directory the last download went to (this session).
    #[cfg(feature = "depot")]
    pub save_dir: Option<String>,
    /// A running download: the file name, and its result (the saved path).
    #[cfg(feature = "depot")]
    pub download: Option<(String, Slot<std::path::PathBuf>)>,
}

impl DepotsView {
    fn depot_count(&self) -> usize {
        self.list.ready().map_or(0, Vec::len)
    }

    /// The highlighted depot.
    pub fn selected_depot(&self) -> Option<&steam::Depot> {
        self.list.ready()?.get(self.selected)
    }
}

impl Browser {
    /// Open the depots overlay for `app_id` at once; the list loads in the
    /// background.
    pub fn open_depots(&mut self, app_id: i32) {
        self.depots.app_id = app_id;
        self.depots.list = Load::start(move || steam::depots(app_id));
        self.depots.selected = 0;
        #[cfg(feature = "depot")]
        {
            self.depots.files = None;
        }
        self.show_depots = true;
    }

    /// Close the depots overlay and drop its contents. A running download
    /// keeps going and reports in the status bar.
    pub fn close_depots(&mut self) {
        self.show_depots = false;
        self.depots.list = Load::default();
        self.depots.selected = 0;
        #[cfg(feature = "depot")]
        {
            self.depots.files = None;
        }
    }

    /// Highlight the next depot (clamped).
    pub fn depots_next(&mut self) {
        if self.depots.selected + 1 < self.depots.depot_count() {
            self.depots.selected += 1;
        }
    }

    /// Highlight the previous depot (clamped).
    pub fn depots_previous(&mut self) {
        self.depots.selected = self.depots.selected.saturating_sub(1);
    }

    /// Adopt finished depot fetches and downloads. Cheap while nothing has
    /// finished; run on every tick.
    pub fn poll_depots(&mut self) {
        if self.depots.list.poll() {
            let max = self.depots.depot_count().saturating_sub(1);
            self.depots.selected = self.depots.selected.min(max);
        }
        #[cfg(feature = "depot")]
        self.poll_depot_files();
    }
}

#[cfg(feature = "depot")]
pub mod files;

#[cfg(test)]
mod tests;
