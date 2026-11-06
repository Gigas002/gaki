//! The per-game settings overlay.

use super::Browser;
use crate::app::steam;

/// The next value in `None → values[0] → … → values[last] → None`.
fn cycle<'a, S: AsRef<str>>(current: Option<&str>, values: &'a [S]) -> Option<&'a str> {
    let next = match current {
        None => 0,
        Some(v) => values
            .iter()
            .position(|x| x.as_ref() == v)
            .map_or(0, |i| i + 1),
    };
    values.get(next).map(AsRef::as_ref)
}

impl Browser {
    /// Number of editable rows in the game-settings overlay: Profile,
    /// Platform, Steam.
    pub const GAME_CONFIG_ROWS: usize = 3;

    /// Fetch the game's settings and open the overlay.
    pub fn open_game_config(&mut self, app_id: i32) {
        self.game_config_app_id = app_id;
        self.game_config_index = 0;
        self.game_config_status.clear();
        self.refresh_game_config();
        self.show_game_config = true;
    }

    pub fn close_game_config(&mut self) {
        self.show_game_config = false;
        self.game_config = None;
        self.game_config_index = 0;
    }

    pub fn game_config_next(&mut self) {
        self.game_config_index = (self.game_config_index + 1) % Self::GAME_CONFIG_ROWS;
    }

    pub fn game_config_previous(&mut self) {
        self.game_config_index = if self.game_config_index == 0 {
            Self::GAME_CONFIG_ROWS - 1
        } else {
            self.game_config_index - 1
        };
    }

    fn refresh_game_config(&mut self) {
        match steam::config_game_show(self.game_config_app_id) {
            Ok(settings) => self.game_config = Some(settings),
            Err(e) => {
                self.game_config = None;
                self.game_config_status = format!("Failed: {e}");
            }
        }
    }

    /// Activate (Enter) the highlighted settings row, cycling its value.
    pub fn game_config_activate(&mut self) {
        let id = self.game_config_app_id;
        let Some(cfg) = self.game_config.clone() else {
            return;
        };
        let (what, next, res) = match self.game_config_index {
            // Profile: default → each profile → default.
            0 => {
                let next = cycle(cfg.profile.as_deref(), &cfg.profiles);
                ("Profile", next, steam::config_game_set_profile(id, next))
            }
            // Platform: auto → windows → linux → auto.
            1 => {
                let next = cycle(cfg.platform.as_deref(), &["windows", "linux"]);
                ("Platform", next, steam::config_game_set_platform(id, next))
            }
            // Steam: the profile's → auto → host → none → the profile's.
            _ => {
                let next = cycle(cfg.steam.as_deref(), &["auto", "host", "none"]);
                ("Steam", next, steam::config_game_set_steam(id, next))
            }
        };
        self.game_config_status = match res {
            Ok(()) => format!("{what} set to {}.", next.unwrap_or("default")),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_game_config();
    }
}

#[cfg(test)]
mod tests;
