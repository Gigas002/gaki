//! The per-game settings overlay.

use super::Browser;
use crate::app::steam;

impl Browser {
    /// Number of editable rows in the game-settings overlay: Forced Proton,
    /// Platform.
    pub const GAME_CONFIG_ROWS: usize = 2;

    /// Fetch the game's per-game config and the installed
    /// Proton list (for cycling the forced runtime) and open the overlay.
    pub fn open_game_config(&mut self, app_id: i32) {
        self.game_config = steam::config_game_show(app_id).ok();
        self.game_config_protons = steam::proton_list()
            .map(|v| {
                v.into_iter()
                    .filter(|p| p.installed)
                    .map(|p| p.name)
                    .collect()
            })
            .unwrap_or_default();
        self.game_config_app_id = app_id;
        self.game_config_index = 0;
        self.game_config_status.clear();
        self.show_game_config = true;
    }

    pub fn close_game_config(&mut self) {
        self.show_game_config = false;
        self.game_config = None;
        self.game_config_protons.clear();
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
        let id = self.game_config_app_id;
        self.game_config = steam::config_game_show(id).ok();
    }

    /// Activate (Enter) the highlighted settings row, cycling its value.
    pub fn game_config_activate(&mut self) {
        let id = self.game_config_app_id;
        match self.game_config_index {
            // Forced Proton: cycle installed runtimes, then wrap to cleared.
            0 => {
                let cur = self
                    .game_config
                    .as_ref()
                    .and_then(|c| c.forced_proton_version.clone());
                let res = if self.game_config_protons.is_empty() {
                    steam::config_game_clear_proton(id)
                } else {
                    let next = match cur.as_deref() {
                        None => Some(0),
                        Some(v) => match self.game_config_protons.iter().position(|p| p == v) {
                            Some(i) if i + 1 < self.game_config_protons.len() => Some(i + 1),
                            _ => None,
                        },
                    };
                    match next {
                        Some(i) => steam::config_game_set_proton(id, &self.game_config_protons[i]),
                        None => steam::config_game_clear_proton(id),
                    }
                };
                self.game_config_status = match res {
                    Ok(()) => "Forced Proton updated.".to_string(),
                    Err(e) => format!("Failed: {e}"),
                };
            }
            // Platform: auto → windows → linux → auto.
            _ => {
                let cur = self
                    .game_config
                    .as_ref()
                    .and_then(|c| c.platform_preference.clone());
                let (next, res) = match cur.as_deref() {
                    None => ("windows", steam::config_game_set_platform(id, "windows")),
                    Some("windows") => ("linux", steam::config_game_set_platform(id, "linux")),
                    Some(_) => ("auto", steam::config_game_clear_platform(id)),
                };
                self.game_config_status = match res {
                    Ok(()) => format!("Platform set to {next}."),
                    Err(e) => format!("Failed: {e}"),
                };
            }
        }
        self.refresh_game_config();
    }
}
