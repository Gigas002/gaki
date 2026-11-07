//! The user profile overlay: `v` on a friend in the Friends panel shows
//! their public profile, as `gaki-cli user` prints it; `/` (in the panel or
//! the overlay) looks anyone up by SteamID64, profile URL, friend name or
//! `me`. `E` opens the profile page in the browser. Read-only.

use libgaki::steam_client::UserProfile;

use super::Browser;
use crate::app::browser::job::Load;
use crate::app::steam;

/// The overlay's state.
#[derive(Default)]
pub struct ProfileView {
    /// What was looked up, as typed (a SteamID64 for `v`).
    pub query: String,
    /// The lookup prompt's text while it's open.
    pub input: Option<String>,
    /// The profile; `None` until something is looked up.
    pub profile: Option<Load<UserProfile>>,
    pub scroll: u16,
}

impl ProfileView {
    /// A view over a loaded `profile`, without fetching (tests, previews).
    pub fn with_profile(profile: UserProfile) -> Self {
        Self {
            query: profile.steam_id.to_string(),
            profile: Some(Load::Ready(profile)),
            ..Self::default()
        }
    }

    pub fn loaded(&self) -> Option<&UserProfile> {
        self.profile.as_ref().and_then(Load::ready)
    }

    fn look_up(&mut self, query: String) {
        self.scroll = 0;
        self.query = query.clone();
        self.profile = Some(Load::start(move || steam::user_profile(&query)));
    }
}

impl Browser {
    /// `v` in the Friends panel: the highlighted friend's profile.
    pub fn open_friend_profile(&mut self) {
        let Some(friend) = self.selected_friend() else {
            return;
        };
        let mut view = ProfileView::default();
        view.look_up(friend.steam_id.to_string());
        self.user_profile = Some(view);
    }

    /// `/` in the Friends panel: the overlay with the lookup prompt open.
    pub fn open_profile_lookup(&mut self) {
        self.user_profile = Some(ProfileView {
            input: Some(String::new()),
            ..ProfileView::default()
        });
    }

    pub fn close_user_profile(&mut self) {
        self.user_profile = None;
    }

    /// `/` in the overlay: open the lookup prompt.
    pub fn begin_profile_lookup(&mut self) {
        if let Some(view) = &mut self.user_profile {
            view.input = Some(String::new());
        }
    }

    pub fn profile_input_open(&self) -> bool {
        self.user_profile
            .as_ref()
            .is_some_and(|v| v.input.is_some())
    }

    pub fn profile_input_push(&mut self, c: char) {
        if let Some(text) = self.user_profile.as_mut().and_then(|v| v.input.as_mut()) {
            text.push(c);
        }
    }

    pub fn profile_input_pop(&mut self) {
        if let Some(text) = self.user_profile.as_mut().and_then(|v| v.input.as_mut()) {
            text.pop();
        }
    }

    /// Esc in the prompt: back to the shown profile, or close the overlay
    /// when nothing was looked up yet.
    pub fn cancel_profile_input(&mut self) {
        let Some(view) = &mut self.user_profile else {
            return;
        };
        view.input = None;
        if view.profile.is_none() {
            self.user_profile = None;
        }
    }

    /// Enter in the prompt: look the text up; empty does nothing.
    pub fn commit_profile_input(&mut self) {
        let Some(view) = &mut self.user_profile else {
            return;
        };
        let Some(text) = view.input.take() else {
            return;
        };
        let query = text.trim().to_string();
        if query.is_empty() {
            if view.profile.is_none() {
                view.input = Some(String::new());
            }
            return;
        }
        view.look_up(query);
    }

    pub fn profile_scroll(&mut self, delta: i16) {
        if let Some(view) = &mut self.user_profile {
            view.scroll = view.scroll.saturating_add_signed(delta);
        }
    }

    /// `E`: open the shown profile's page in the browser.
    pub fn open_profile_page(&mut self) {
        let Some(url) = self
            .user_profile
            .as_ref()
            .and_then(ProfileView::loaded)
            .map(|p| p.profile_url.clone())
            .filter(|u| !u.is_empty())
        else {
            return;
        };
        self.open_url(url, "the Steam profile");
    }

    /// Adopt a finished lookup. Called once per event-loop iteration.
    pub fn poll_user_profile(&mut self) {
        if let Some(load) = self.user_profile.as_mut().and_then(|v| v.profile.as_mut()) {
            load.poll();
        }
    }
}

#[cfg(test)]
mod tests;
