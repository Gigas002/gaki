//! The TUI's own settings file: the default user, hidden and favourite
//! games, the shown game types, the highlight colour, and the background
//! update check, in
//! `gaki/tui/config.toml` under the config dir.

use crate::utils::error::TuiError;
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
/// Avoids a `config.toml` disk read + parse on every `get_name`/`is_valid`
/// call (which the list filter runs for every game, several times per keypress).
static CACHE: RwLock<Option<Config>> = RwLock::new(None);

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub default_user: String,
    pub hidden_games: Vec<i32>,
    pub favorite_games: Vec<i32>,
    pub allowed_games: Vec<GameType>,
    pub highlight: Color,
    /// Check the installed games for updates in the background after the
    /// library loads.
    pub check_updates: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            default_user: "".to_string(),
            hidden_games: vec![],
            favorite_games: vec![],
            allowed_games: vec![GameType::Game, GameType::DLC],
            highlight: theme::ACCENT,
            check_updates: true,
        }
    }
}

impl Config {
    /// Read the settings file. A missing or empty one is the defaults; a
    /// broken one is logged and the defaults are used, leaving the file alone
    /// until the next change is saved.
    pub fn new() -> Result<Config, TuiError> {
        let path = config_location()?;
        let config = Self::parse(&fs::read_to_string(&path)?).unwrap_or_else(|e| {
            tracing::warn!("ignoring {}: {e}", path.display());
            Config::default()
        });
        *CACHE.write()? = Some(config.clone());
        Ok(config)
    }

    fn parse(text: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(text)
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

    pub fn save(&self) -> Result<(), TuiError> {
        // Publish only persisted settings. A failed save must not make a toggle
        // appear successful in cached readers while the UI retains its old copy.
        let text = toml::to_string(&self).map_err(|e| TuiError::Problem(e.to_string()))?;
        libgaki::config::edit::write_atomic(&config_location()?, &text)
            .map_err(|e| TuiError::Problem(format!("{e:#}")))?;
        *CACHE.write()? = Some(self.clone());
        Ok(())
    }
}
