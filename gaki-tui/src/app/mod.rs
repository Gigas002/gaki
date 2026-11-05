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
    /// The view waiting for a fresh login (`reauth_sensitive`); it opens once
    /// the login finishes and the library is back.
    pub reauth: Option<SensitiveView>,
}

/// A view that reads sensitive account data: with `reauth_sensitive` on, it
/// opens only after a fresh login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveView {
    Wallet,
    /// The inventory for this app id.
    Inventory(i32),
    Listings,
}

impl SensitiveView {
    pub fn label(self) -> &'static str {
        match self {
            SensitiveView::Wallet => "wallet",
            SensitiveView::Inventory(_) => "inventory",
            SensitiveView::Listings => "market listings",
        }
    }

    fn open(self, browser: &mut browser::Browser) {
        match self {
            SensitiveView::Wallet => {
                if let Err(e) = browser.open_wallet() {
                    browser.notice = Some(format!("Wallet: {e}"));
                }
            }
            SensitiveView::Inventory(app_id) => browser.open_inventory(app_id),
            SensitiveView::Listings => browser.open_market(),
        }
    }
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
            reauth: None,
        }
    }

    /// Open `view`, or, when it needs a fresh login first, go to the sign-in
    /// page and open it afterwards.
    pub fn open_sensitive(&mut self, browser: &mut browser::Browser, view: SensitiveView) {
        if libgaki::ops::auth::reauth_needed() {
            self.reauth = Some(view);
            self.mode = Mode::Login;
        } else {
            view.open(browser);
        }
    }

    /// Back in the library: open the view a login was for, if there is one.
    /// Without a fresh login (the sign-in was skipped) it stays closed.
    pub fn resume_sensitive(&mut self, browser: &mut browser::Browser) {
        if self.mode != Mode::Browse {
            return;
        }
        let Some(view) = self.reauth.take() else {
            return;
        };
        if libgaki::ops::auth::reauth_needed() {
            browser.notice = Some(format!(
                "The {} needs a fresh sign-in (reauth_sensitive).",
                view.label()
            ));
        } else {
            view.open(browser);
        }
    }
}
