//! Application state, the run loops, and the `libgaki` worker ([`worker`]).

mod artwork;
pub mod browser;
pub mod chat;
pub mod client;
pub mod game;
mod keys;
pub mod library;
mod marquee;
pub mod steam;
mod terminal;
pub mod worker;

/// What the run loops return. Their errors come from the terminal, the
/// config file, and the client, so they're boxed.
pub type AppResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

use crate::config::Config;

use ratatui::style::Color;

pub struct App {
    pub mode: Mode,
    pub user: String,
    /// In-progress password entry (classic login). Never persisted.
    pub password: String,
    /// In-progress Steam Guard code entry.
    pub guard_code: String,
    pub highlight: Color,
}

#[derive(PartialEq, Clone)]
pub enum Mode {
    Login,
    LoginQr,
    /// Typing the Steam username (classic login).
    LoginUser,
    /// Typing the Steam password (classic login).
    LoginPass,
    /// Classic login in progress (connecting / awaiting confirmation).
    LoginClassic,
    /// Typing a Steam Guard code prompted during classic login.
    LoginGuard,
    Loading,
    /// The main library browser (filter tabs, list, detail, status bar).
    Browse,
    Failed,
    Terminated(String),
}

impl App {
    pub fn new(config: &Config) -> App {
        let user = config.default_user.clone();
        let highlight = config.highlight;
        App {
            mode: if user.is_empty() {
                Mode::Login
            } else {
                Mode::Loading
            },
            user,
            password: String::new(),
            guard_code: String::new(),
            highlight,
        }
    }
}
