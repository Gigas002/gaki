//! Web pages about the selected game, opened in the default browser.

use std::thread;

use libgaki::core::steam_urls;

use super::Browser;
use crate::utils::browser;

/// A page about one game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Its ProtonDB page.
    ProtonDb,
    /// A search of Valve's Proton issue tracker for its app id.
    ProtonIssues,
}

impl Link {
    /// What the notice calls the page.
    pub fn label(self) -> &'static str {
        match self {
            Link::ProtonDb => "ProtonDB",
            Link::ProtonIssues => "Proton issues",
        }
    }

    /// The page's URL for `app_id`.
    pub fn url(self, app_id: i32) -> String {
        let app_id = app_id as u32;
        match self {
            Link::ProtonDb => steam_urls::protondb_url(app_id),
            Link::ProtonIssues => steam_urls::proton_issues_url(app_id),
        }
    }
}

impl Browser {
    /// Open `link` for `app_id` in the default browser. The launcher can
    /// block, so it runs on its own thread; a failure comes back through
    /// [`Browser::poll_links`].
    pub fn open_link(&mut self, link: Link, app_id: i32) {
        let url = link.url(app_id);
        let slot = self.link_error.clone();
        thread::spawn(move || {
            if let Err(e) = browser::open(&url) {
                tracing::warn!(%url, error = %e, "couldn't open the browser");
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(format!("Couldn't open {url}: {e}"));
                }
            }
        });
        self.notice = Some(format!("Opening {} in your browser…", link.label()));
    }

    /// Show a failed [`Browser::open_link`] as the notice.
    pub fn poll_links(&mut self) {
        let failed = self.link_error.lock().ok().and_then(|mut slot| slot.take());
        if let Some(message) = failed {
            self.notice = Some(message);
        }
    }
}
