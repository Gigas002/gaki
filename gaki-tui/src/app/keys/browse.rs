//! Keys in the library itself. Overlays and the Actions menu take keys
//! first; the rest is the base keymap, looked up in [`keymap`] so
//! `keybinds.toml` can rebind it.

use ratatui::crossterm::event::KeyCode;

use super::{Flow, actions, overlays, shared};
#[cfg(feature = "inventory")]
use crate::app::SensitiveView;
use crate::app::browser::{Filter, Link};
use crate::app::library::Library;
use crate::app::{AppResult, Mode};
use crate::keymap::{self, Key, LibraryAction as A};

pub(super) fn key(lib: &mut Library, key: Key) -> AppResult<Flow> {
    // Any keypress dismisses the previous action's notice (this action sets
    // it again if it fails too).
    lib.browser.notice = None;
    if lib.browser.show_actions {
        actions::key(lib, key)?;
    } else if !overlays::key(lib, key)? {
        return base(lib, key);
    }
    Ok(Flow::Continue)
}

/// The base keymap, with no overlay open.
fn base(lib: &mut Library, key: Key) -> AppResult<Flow> {
    let Library {
        app,
        browser,
        client,
        config,
        cached,
        ..
    } = lib;
    // The fixed keys first: `?` opens the help, the arrows move.
    match key.code {
        KeyCode::Char('?') if !key.ctrl && !key.alt => {
            browser.help_scroll = 0;
            browser.show_help = true;
            return Ok(Flow::Continue);
        }
        KeyCode::Down if !key.ctrl && !key.alt => {
            browser.next();
            return Ok(Flow::Continue);
        }
        KeyCode::Up if !key.ctrl && !key.alt => {
            browser.previous();
            return Ok(Flow::Continue);
        }
        _ => {}
    }
    match keymap::get().library_action(key) {
        Some(A::Quit) => return Ok(Flow::Quit),
        Some(A::Account) => {
            // Fetch the account (blocking) and open the overlay;
            // ignore failures so a missing session is non-fatal.
            let _ = browser.open_account();
        }
        Some(A::Settings) => {
            // Settings: reads config.toml (local only).
            browser.open_config();
        }
        #[cfg(feature = "inventory")]
        Some(A::Wallet) => {
            // Fetch the wallet (blocking) and open the overlay; a
            // failure becomes a notice.
            app.open_sensitive(browser, SensitiveView::Wallet);
        }
        Some(A::Filter) => browser.filtering = true,
        Some(A::SignIn) => {
            app.mode = Mode::Login;
        }
        Some(A::Reload) => shared::reload(app, client, cached)?,
        Some(A::NextTab) => browser.cycle_filter(true),
        Some(A::PreviousTab) => browser.cycle_filter(false),
        Some(A::TabAll) => browser.set_filter(Filter::All),
        Some(A::TabInstalled) => browser.set_filter(Filter::Installed),
        Some(A::TabUpdates) => browser.set_filter(Filter::Updates),
        Some(A::TabFavourites) => browser.set_filter(Filter::Favourites),
        #[cfg(feature = "social")]
        Some(A::TabFriends) => browser.enter_friends(),
        Some(A::Sort) => browser.cycle_sort(),
        Some(A::Achievements) => browser.open_achievements(),
        #[cfg(feature = "social")]
        Some(A::Friends) => browser.toggle_friends_focus(),
        #[cfg(feature = "inventory")]
        Some(A::Inventory) => {
            if let Some(game) = browser.selected() {
                app.open_sensitive(browser, SensitiveView::Inventory(game.id));
            }
        }
        #[cfg(feature = "market")]
        Some(A::MarketListings) => app.open_sensitive(browser, SensitiveView::Listings),
        #[cfg(feature = "market")]
        Some(A::MarketSearch) => browser.open_market_search(),
        #[cfg(feature = "workshop")]
        Some(A::Workshop) => {
            if let Some(game) = browser.selected() {
                browser.open_workshop(game.id);
            }
        }
        Some(A::Description) => browser.toggle_description(),
        Some(A::Down) => browser.next(),
        Some(A::Up) => browser.previous(),
        Some(A::Top) => browser.home(),
        Some(A::Bottom) => browser.end(),
        Some(A::PageDown) => browser.page_down(10),
        Some(A::PageUp) => browser.page_up(10),
        Some(A::ActionsMenu) => {
            // Open the per-game Actions menu (command palette).
            // Power users keep the direct accelerators below;
            // the menu is the discoverable path to all of them
            // (Play is the default-highlighted row).
            browser.open_actions();
        }
        #[cfg(feature = "versioning")]
        Some(A::Versions) => {
            if let Some(game) = browser.selected() {
                browser.open_versions(game.id);
            }
        }
        #[cfg(feature = "collections")]
        Some(A::Collections) => {
            if let Some(game) = browser.selected() {
                browser.open_collections(game.id);
            }
        }
        #[cfg(feature = "install")]
        Some(A::Install) => {
            if let Some(game) = browser.selected() {
                shared::install(browser, client, &game)?;
            }
        }
        #[cfg(feature = "install")]
        Some(A::PauseResume) => {
            if let Some(game) = browser.selected() {
                shared::pause_or_resume(browser, client, &game)?;
            }
        }
        #[cfg(feature = "install")]
        Some(A::CancelInstall) => {
            if let Some(game) = browser.selected() {
                shared::cancel_install(browser, &game);
            }
        }
        #[cfg(feature = "install")]
        Some(A::Uninstall) => {
            // Only offer uninstall for installed games.
            if let Some(game) = browser.selected()
                && game.installed
            {
                browser.confirm_uninstall = true;
            }
        }
        #[cfg(feature = "install")]
        Some(A::MoveInstall) => {
            // Only offer move for installed games.
            if let Some(game) = browser.selected()
                && game.installed
            {
                browser.open_move(game.id);
            }
        }
        #[cfg(feature = "install")]
        Some(A::Relink) => {
            // Only offer relink for installed games.
            if let Some(game) = browser.selected()
                && game.installed
            {
                browser.open_relink(game.id);
            }
        }
        #[cfg(feature = "install")]
        Some(A::Import) => {
            // Import registers an on-disk install, so it does
            // not require the game to already be installed.
            if let Some(game) = browser.selected() {
                browser.open_import(game.id);
            }
        }
        #[cfg(feature = "install")]
        Some(A::Verify) => {
            if let Some(game) = browser.selected() {
                client.verify(&game)?;
            }
        }
        #[cfg(feature = "install")]
        Some(A::Update) => {
            if let Some(game) = browser.selected() {
                client.update(&game)?;
            }
        }
        Some(A::Dlc) => {
            if let Some(game) = browser.selected() {
                // Blocking fetch; failure leaves the overlay closed.
                let _ = browser.open_dlc(game.id);
            }
        }
        #[cfg(feature = "versioning")]
        Some(A::Branches) => {
            if let Some(game) = browser.selected() {
                // Blocking fetch; failure leaves the overlay closed.
                let _ = browser.open_branches(game.id);
            }
        }
        Some(A::Proton) => {
            // Blocking fetch; failure leaves the overlay closed.
            let _ = browser.open_proton();
        }
        Some(A::Running) => browser.open_running(),
        Some(A::Favourite) => {
            if let Some(game) = browser.selected() {
                shared::toggle_favourite(browser, config, &game)?;
            }
        }
        Some(A::Hide) => {
            if let Some(game) = browser.selected() {
                shared::hide(browser, config, &game)?;
            }
        }
        #[cfg(feature = "cloud")]
        Some(A::Cloud) => {
            if let Some(game) = browser.selected() {
                browser.open_cloud(game.id);
            }
        }
        Some(A::Depots) => {
            if let Some(game) = browser.selected() {
                browser.open_depots(game.id);
            }
        }
        #[cfg(feature = "store")]
        Some(A::SteamStore) => browser.open_shop(),
        #[cfg(feature = "install")]
        Some(A::Installs) => browser.open_installs(),
        #[cfg(feature = "store")]
        Some(A::Store) => {
            if let Some(game) = browser.selected() {
                browser.open_store(game.id, &game.name);
            }
        }
        Some(A::SteamPage) => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::Steam, game.id);
            }
        }
        Some(A::ProtonDb) => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::ProtonDb, game.id);
            }
        }
        Some(A::ProtonIssues) => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::ProtonIssues, game.id);
            }
        }
        Some(A::GameInfo) => {
            if let Some(game) = browser.selected() {
                browser.open_game_info(game.id, &game.name);
            }
        }
        Some(A::SteamDb) => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::SteamDb, game.id);
            }
        }
        Some(A::LaunchOptions) => {
            if let Some(game) = browser.selected() {
                browser.open_launch(game.id);
            }
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
