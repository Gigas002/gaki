//! Web pages about the selected game (or one of its depots), opened in the
//! default browser.

use std::thread;

use libgaki::core::steam_urls;

use super::Browser;
use crate::utils::browser;

/// A page about one game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Its Steam store page.
    Steam,
    /// Its ProtonDB page.
    ProtonDb,
    /// A search of Valve's Proton issue tracker for its app id.
    ProtonIssues,
    /// Its SteamDB page.
    SteamDb,
    /// A depot's manifest history on SteamDB; the id is the depot's.
    SteamDbDepot,
}

impl Link {
    /// What the notice calls the page.
    pub fn label(self) -> &'static str {
        match self {
            Link::Steam => "the Steam page",
            Link::ProtonDb => "ProtonDB",
            Link::ProtonIssues => "Proton issues",
            Link::SteamDb => "SteamDB",
            Link::SteamDbDepot => "the depot on SteamDB",
        }
    }

    /// The page's URL for `id`: an app id, or a depot id for
    /// [`Link::SteamDbDepot`].
    pub fn url(self, id: i32) -> String {
        let id = id as u32;
        match self {
            Link::Steam => steam_urls::store_url(id),
            Link::ProtonDb => steam_urls::protondb_url(id),
            Link::ProtonIssues => steam_urls::proton_issues_url(id),
            Link::SteamDb => steam_urls::steamdb_url(id),
            Link::SteamDbDepot => steam_urls::steamdb_depot_url(id),
        }
    }
}

impl Browser {
    /// Open `link` for `app_id` (a depot id for [`Link::SteamDbDepot`]) in
    /// the default browser. The launcher can
    /// block, so it runs on its own thread; a failure comes back through
    /// [`Browser::poll_links`].
    pub fn open_link(&mut self, link: Link, app_id: i32) {
        self.open_url(link.url(app_id), link.label());
    }

    /// Open `url` in the default browser on its own thread; `label` names
    /// it in the notice.
    pub fn open_url(&mut self, url: String, label: &str) {
        let slot = self.link_error.clone();
        thread::spawn(move || {
            if let Err(e) = browser::open(&url) {
                tracing::warn!(%url, error = %e, "couldn't open the browser");
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(format!("Couldn't open {url}: {e}"));
                }
            }
        });
        self.notice = Some(format!("Opening {label} in your browser…"));
    }

    /// Show a failed [`Browser::open_link`] as the notice.
    pub fn poll_links(&mut self) {
        let failed = self.link_error.lock().ok().and_then(|mut slot| slot.take());
        if let Some(message) = failed {
            self.notice = Some(message);
        }
    }
}
