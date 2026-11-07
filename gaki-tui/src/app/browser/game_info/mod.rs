//! The Game info overlay (`n`): the selected game's full store record
//! (Details), its DRM tickets and files on disk (Checks), and saving its
//! artwork (Artwork). Everything loads in the background.

use super::Browser;
use crate::app::browser::job::Load;
use crate::app::steam;
use crate::utils::xdg;

/// The overlay's tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InfoTab {
    #[default]
    Details,
    Checks,
    Artwork,
}

impl InfoTab {
    pub const ALL: [InfoTab; 3] = [InfoTab::Details, InfoTab::Checks, InfoTab::Artwork];

    pub fn label(self) -> &'static str {
        match self {
            InfoTab::Details => "Details",
            InfoTab::Checks => "Checks",
            InfoTab::Artwork => "Artwork",
        }
    }

    fn step(self, forward: bool) -> InfoTab {
        let i = InfoTab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let n = InfoTab::ALL.len();
        InfoTab::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// The overlay's state.
pub struct GameInfoView {
    pub app_id: i32,
    pub name: String,
    pub tab: InfoTab,
    /// The shown tab's scroll, in rows.
    pub scroll: u16,
    pub details: Load<steam::GameInfo>,
    /// Run the first time Checks is shown, and on `r`.
    pub drm: Option<Load<steam::DrmResult>>,
    #[cfg(feature = "install")]
    pub files: Option<Load<steam::FilesCheck>>,
    /// Where `s` saves the artwork.
    pub save_dir: String,
    /// The folder prompt's text while editing it (`o`).
    pub dir_input: Option<String>,
    pub saving: Option<Load<Vec<steam::SavedImage>>>,
}

impl GameInfoView {
    /// A view over already-loaded details (tests, previews).
    pub fn with_details(app_id: i32, name: &str, details: steam::GameInfo) -> Self {
        Self {
            app_id,
            name: name.to_string(),
            tab: InfoTab::Details,
            scroll: 0,
            details: Load::Ready(details),
            drm: None,
            #[cfg(feature = "install")]
            files: None,
            save_dir: xdg::user_dir("PICTURES", "Pictures"),
            dir_input: None,
            saving: None,
        }
    }

    fn run_checks(&mut self) {
        let id = self.app_id;
        self.drm = Some(Load::start(move || steam::drm_check(id)));
        #[cfg(feature = "install")]
        {
            self.files = Some(Load::start(move || steam::files_check(id)));
        }
    }
}

impl Browser {
    /// Open the overlay for a game; its store record loads in the background.
    pub fn open_game_info(&mut self, app_id: i32, name: &str) {
        let mut view = GameInfoView::with_details(app_id, name, placeholder());
        view.details = Load::start(move || steam::game_info(app_id));
        self.game_info = Some(view);
    }

    pub fn close_game_info(&mut self) {
        self.game_info = None;
    }

    /// `Tab` / `Shift-Tab`. The checks run the first time Checks is shown.
    pub fn game_info_tab(&mut self, forward: bool) {
        let Some(view) = &mut self.game_info else {
            return;
        };
        view.tab = view.tab.step(forward);
        view.scroll = 0;
        if view.tab == InfoTab::Checks && view.drm.is_none() {
            view.run_checks();
        }
    }

    pub fn game_info_scroll(&mut self, delta: i16) {
        if let Some(view) = &mut self.game_info {
            view.scroll = view.scroll.saturating_add_signed(delta);
        }
    }

    /// `r` in Checks: check again.
    pub fn rerun_game_checks(&mut self) {
        if let Some(view) = &mut self.game_info
            && !view.drm.as_ref().is_some_and(Load::is_loading)
        {
            view.run_checks();
        }
    }

    /// `o` in Artwork: edit the folder.
    pub fn begin_artwork_dir(&mut self) {
        if let Some(view) = &mut self.game_info {
            view.dir_input = Some(view.save_dir.clone());
        }
    }

    pub fn artwork_dir_push(&mut self, c: char) {
        if let Some(text) = self.game_info.as_mut().and_then(|v| v.dir_input.as_mut()) {
            text.push(c);
        }
    }

    pub fn artwork_dir_pop(&mut self) {
        if let Some(text) = self.game_info.as_mut().and_then(|v| v.dir_input.as_mut()) {
            text.pop();
        }
    }

    pub fn cancel_artwork_dir(&mut self) {
        if let Some(view) = &mut self.game_info {
            view.dir_input = None;
        }
    }

    pub fn commit_artwork_dir(&mut self) {
        if let Some(view) = &mut self.game_info
            && let Some(text) = view.dir_input.take()
            && !text.trim().is_empty()
        {
            view.save_dir = text.trim().to_string();
        }
    }

    /// `s` in Artwork: save the images into the folder, in the background.
    /// Existing files are left alone.
    pub fn save_game_artwork(&mut self) {
        let Some(view) = &mut self.game_info else {
            return;
        };
        if view.saving.as_ref().is_some_and(Load::is_loading) {
            return;
        }
        let home = libgaki::core::config::home_dir().unwrap_or_default();
        let dir = xdg::expand_dir(&view.save_dir, &home);
        let id = view.app_id;
        view.saving = Some(Load::start(move || steam::save_artwork(id, dir)));
    }

    /// Adopt finished loads. Called once per tick.
    pub fn poll_game_info(&mut self) {
        let Some(view) = &mut self.game_info else {
            return;
        };
        view.details.poll();
        if let Some(drm) = &mut view.drm {
            drm.poll();
        }
        #[cfg(feature = "install")]
        if let Some(files) = &mut view.files {
            files.poll();
        }
        if let Some(saving) = &mut view.saving {
            saving.poll();
        }
    }
}

/// An empty record, replaced as soon as the real one loads.
fn placeholder() -> steam::GameInfo {
    steam::GameInfo {
        details: Default::default(),
        extended: None,
        dlc_count: 0,
    }
}

#[cfg(test)]
mod tests;
