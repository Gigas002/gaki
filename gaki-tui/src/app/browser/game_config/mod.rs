//! The per-game settings overlay.

use std::collections::BTreeMap;

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

/// Split typed text into words like a shell: whitespace separates them,
/// `"…"` and `'…'` keep spaces, and `\` escapes the next character (outside
/// single quotes).
pub(crate) fn split_words(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    // A word is open even when it's empty, e.g. `""`.
    let mut open = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), c) => word.push(c),
            (_, '\\') => {
                let next = chars.next().ok_or("a trailing `\\` escapes nothing")?;
                word.push(next);
                open = true;
            }
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                open = true;
            }
            (None, c) if c.is_whitespace() => {
                if open {
                    words.push(std::mem::take(&mut word));
                    open = false;
                }
            }
            (None, c) => {
                word.push(c);
                open = true;
            }
        }
    }
    if let Some(q) = quote {
        return Err(format!("unclosed {q}"));
    }
    if open {
        words.push(word);
    }
    Ok(words)
}

/// Join words so that [`split_words`] gives them back.
pub(crate) fn join_words<S: AsRef<str>>(words: &[S]) -> String {
    words
        .iter()
        .map(|w| {
            let w = w.as_ref();
            let plain = !w.is_empty()
                && !w
                    .chars()
                    .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'));
            if plain {
                w.to_string()
            } else {
                let escaped = w.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse typed `KEY=VALUE` words into launch variables.
pub(crate) fn parse_env(text: &str) -> Result<BTreeMap<String, String>, String> {
    split_words(text)?
        .iter()
        .map(|w| libgaki::ops::config::parse_env_var(w).map_err(|e| format!("{e:#}")))
        .collect()
}

/// The overlay's rows, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameConfigRow {
    Profile,
    Platform,
    Steam,
    Args,
    Env,
    ShaderCache,
}

impl GameConfigRow {
    pub const ALL: [Self; 6] = [
        Self::Profile,
        Self::Platform,
        Self::Steam,
        Self::Args,
        Self::Env,
        Self::ShaderCache,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Platform => "Platform",
            Self::Steam => "Steam",
            Self::Args => "Args",
            Self::Env => "Env",
            Self::ShaderCache => "Shader cache",
        }
    }

    /// The row is typed, not cycled.
    pub fn is_text(self) -> bool {
        matches!(self, Self::Args | Self::Env | Self::ShaderCache)
    }
}

impl Browser {
    /// Number of editable rows in the game-settings overlay.
    pub const GAME_CONFIG_ROWS: usize = GameConfigRow::ALL.len();

    /// The highlighted row.
    pub fn game_config_row(&self) -> GameConfigRow {
        GameConfigRow::ALL[self.game_config_index.min(Self::GAME_CONFIG_ROWS - 1)]
    }

    /// Fetch the game's settings and open the overlay.
    pub fn open_game_config(&mut self, app_id: i32) {
        self.game_config_app_id = app_id;
        self.game_config_index = 0;
        self.game_config_input = None;
        self.game_config_status.clear();
        self.refresh_game_config();
        self.show_game_config = true;
    }

    pub fn close_game_config(&mut self) {
        self.show_game_config = false;
        self.game_config = None;
        self.game_config_index = 0;
        self.game_config_input = None;
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

    /// Activate (Enter) the highlighted settings row: cycle its value, or
    /// start typing it.
    pub fn game_config_activate(&mut self) {
        let id = self.game_config_app_id;
        let Some(cfg) = self.game_config.clone() else {
            return;
        };
        let row = self.game_config_row();
        if row.is_text() {
            self.game_config_input = Some(match row {
                GameConfigRow::Args => join_words(&cfg.args),
                GameConfigRow::Env => join_words(
                    &cfg.env
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>(),
                ),
                _ => cfg.shader_cache.clone().unwrap_or_default(),
            });
            self.game_config_status.clear();
            return;
        }
        let (next, res) = match row {
            // Profile: default → each profile → default.
            GameConfigRow::Profile => {
                let next = cycle(cfg.profile.as_deref(), &cfg.profiles);
                (next, steam::config_game_set_profile(id, next))
            }
            // Platform: auto → windows → linux → auto.
            GameConfigRow::Platform => {
                let next = cycle(cfg.platform.as_deref(), &["windows", "linux"]);
                (next, steam::config_game_set_platform(id, next))
            }
            // Steam: the profile's → auto → host → none → the profile's.
            _ => {
                let next = cycle(cfg.steam.as_deref(), &["auto", "host", "none"]);
                (next, steam::config_game_set_steam(id, next))
            }
        };
        self.game_config_status = match res {
            Ok(()) => format!("{} set to {}.", row.label(), next.unwrap_or("default")),
            Err(e) => format!("Failed: {e}"),
        };
        self.refresh_game_config();
    }

    pub fn game_config_input_push(&mut self, c: char) {
        if let Some(input) = &mut self.game_config_input {
            input.push(c);
        }
    }

    pub fn game_config_input_pop(&mut self) {
        if let Some(input) = &mut self.game_config_input {
            input.pop();
        }
    }

    pub fn game_config_input_cancel(&mut self) {
        self.game_config_input = None;
    }

    /// `Enter` while typing: save the typed value. Empty text clears the
    /// setting; text that doesn't parse keeps the prompt open.
    pub fn game_config_input_submit(&mut self) {
        let Some(text) = self.game_config_input.clone() else {
            return;
        };
        let id = self.game_config_app_id;
        let row = self.game_config_row();
        let text = text.trim();
        let res = match row {
            GameConfigRow::Args => split_words(text)
                .map(|args| steam::config_game_set_args(id, args).map_err(|e| e.to_string())),
            GameConfigRow::Env => parse_env(text)
                .map(|env| steam::config_game_set_env(id, env).map_err(|e| e.to_string())),
            _ => Ok(
                steam::config_game_set_shader_cache(id, (!text.is_empty()).then_some(text))
                    .map_err(|e| e.to_string()),
            ),
        };
        self.game_config_status = match res {
            // Bad input: say why and keep typing.
            Err(e) => {
                self.game_config_status = format!("Invalid: {e}");
                return;
            }
            Ok(Err(e)) => format!("Failed: {e}"),
            Ok(Ok(())) if text.is_empty() => format!("{} cleared.", row.label()),
            Ok(Ok(())) => format!("{} saved.", row.label()),
        };
        self.game_config_input = None;
        self.refresh_game_config();
    }
}

#[cfg(test)]
mod tests;
