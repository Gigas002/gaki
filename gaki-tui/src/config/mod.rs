//! The TUI's own settings file: the default user, hidden and favourite
//! games, the shown game types, and the highlight colour, in
//! `gaki/tui/config.json` under the config dir. Phase 5 turns it into TOML.

use crate::utils::error::STError;
use crate::utils::paths::config_location;

use crate::app::game::GameType;

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::RwLock;

use ratatui::style::Color;

use crate::ui::theme;

#[cfg(test)]
mod tests;

/// Process-wide cache of the parsed config. `Config::new`/`save` keep it in sync.
/// Avoids a `config.json` disk read + JSON parse on every `get_name`/`is_valid`
/// call (which the list filter runs for every game, several times per keypress).
static CACHE: RwLock<Option<Config>> = RwLock::new(None);

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub default_user: String,
    pub hidden_games: Vec<i32>,
    pub favorite_games: Vec<i32>,
    pub allowed_games: Vec<GameType>,
    pub highlight: Color,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            default_user: "".to_string(),
            hidden_games: vec![],
            favorite_games: vec![],
            allowed_games: vec![GameType::Game, GameType::DLC],
            highlight: theme::ACCENT,
        }
    }
}

impl Config {
    pub fn new() -> Result<Config, STError> {
        let config = match serde_json::from_str(&fs::read_to_string(config_location()?)?) {
            Ok(config) => config,
            _ => {
                let config = Config::default();
                config.save()?;
                config
            }
        };
        *CACHE.write()? = Some(config.clone());
        Ok(config)
    }

    /// Cheap, cached read of the config. Loads from disk only on the first call;
    /// thereafter serves a clone of the in-memory copy. Hot path for the list
    /// rendering/filtering, so it must never touch the filesystem on repeat.
    pub fn cached() -> Config {
        {
            let guard = CACHE.read().expect("config cache poisoned");
            if let Some(config) = &*guard {
                return config.clone();
            }
        }
        Config::new().unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), STError> {
        // Publish only persisted settings. A failed save must not make a toggle
        // appear successful in cached readers while the UI retains its old copy.
        fs::write(config_location()?, serde_json::to_string(&self)?)?;
        *CACHE.write()? = Some(self.clone());
        Ok(())
    }
}
