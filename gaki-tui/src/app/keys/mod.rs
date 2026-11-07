//! Key handling for the library window, split by screen.

mod actions;
mod browse;
mod login;
mod overlays;
mod shared;

use ratatui::crossterm::event::KeyCode;

use crate::app::library::Library;
use crate::app::{AppResult, Mode};
use crate::keymap::Key;

/// Whether the loop keeps going after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Flow {
    Continue,
    Quit,
}

impl Library {
    /// Handle one key for the current screen.
    pub(super) fn handle_key(&mut self, key: Key) -> AppResult<Flow> {
        // Ctrl+C quits from anywhere; it can't be rebound.
        if key.is_interrupt() {
            return Ok(Flow::Quit);
        }
        let input = key.code;
        let flow = match self.app.mode {
            Mode::Terminated(_) if input == KeyCode::Char('q') => Flow::Quit,
            Mode::Terminated(_) | Mode::Loading => Flow::Continue,
            Mode::Browse => browse::key(self, key)?,
            Mode::Login
            | Mode::Failed
            | Mode::LoginQr
            | Mode::LoginUser
            | Mode::LoginPass
            | Mode::LoginClassic
            | Mode::LoginGuard => login::key(self, input)?,
        };
        // `q` also leaves the loading screen, including one a key just opened.
        if flow == Flow::Quit || (self.app.mode == Mode::Loading && input == KeyCode::Char('q')) {
            return Ok(Flow::Quit);
        }
        Ok(Flow::Continue)
    }
}
