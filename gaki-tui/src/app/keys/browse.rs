//! Keys in the library itself. Overlays and the Actions menu take keys
//! first; the rest is the base keymap.

use ratatui::crossterm::event::KeyCode;

use super::{Flow, actions, overlays, shared};
#[cfg(feature = "inventory")]
use crate::app::SensitiveView;
use crate::app::browser::{Filter, Link};
use crate::app::library::Library;
use crate::app::{AppResult, Mode};

pub(super) fn key(lib: &mut Library, input: KeyCode) -> AppResult<Flow> {
    // Any keypress dismisses the previous action's notice (this action sets
    // it again if it fails too).
    lib.browser.notice = None;
    if lib.browser.show_actions {
        actions::key(lib, input)?;
    } else if !overlays::key(lib, input)? {
        return base(lib, input);
    }
    Ok(Flow::Continue)
}

/// The base keymap, with no overlay open.
fn base(lib: &mut Library, input: KeyCode) -> AppResult<Flow> {
    let Library {
        app,
        browser,
        client,
        config,
        cached,
        ..
    } = lib;
    match input {
        KeyCode::Char('q') => return Ok(Flow::Quit),
        KeyCode::Char('?') => {
            browser.help_scroll = 0;
            browser.show_help = true;
        }
        KeyCode::Char('A') => {
            // Fetch the account (blocking) and open the overlay;
            // ignore failures so a missing session is non-fatal.
            let _ = browser.open_account();
        }
        KeyCode::Char('p') => {
            // Fetch the launcher configuration (blocking) and
            // open the settings overlay; ignore failures.
            let _ = browser.open_config();
        }
        #[cfg(feature = "inventory")]
        KeyCode::Char('w') => {
            // Fetch the wallet (blocking) and open the overlay; a
            // failure becomes a notice.
            app.open_sensitive(browser, SensitiveView::Wallet);
        }
        KeyCode::Char('/') => browser.filtering = true,
        KeyCode::Char('l') => {
            app.mode = Mode::Login;
        }
        KeyCode::Char('r') => shared::reload(app, client, cached)?,
        KeyCode::Tab => browser.cycle_filter(true),
        KeyCode::BackTab => browser.cycle_filter(false),
        KeyCode::Char('1') => browser.set_filter(Filter::All),
        KeyCode::Char('2') => browser.set_filter(Filter::Installed),
        KeyCode::Char('3') => browser.set_filter(Filter::Updates),
        KeyCode::Char('4') => browser.set_filter(Filter::Favourites),
        #[cfg(feature = "social")]
        KeyCode::Char('5') => browser.enter_friends(),
        KeyCode::Char('s') => browser.cycle_sort(),
        KeyCode::Char('a') => browser.open_achievements(),
        #[cfg(feature = "social")]
        KeyCode::Char('F') => browser.toggle_friends_focus(),
        #[cfg(feature = "inventory")]
        KeyCode::Char('I') => {
            if let Some(game) = browser.selected() {
                app.open_sensitive(browser, SensitiveView::Inventory(game.id));
            }
        }
        #[cfg(feature = "market")]
        KeyCode::Char('m') => app.open_sensitive(browser, SensitiveView::Listings),
        #[cfg(feature = "market")]
        KeyCode::Char('S') => browser.open_market_search(),
        #[cfg(feature = "workshop")]
        KeyCode::Char('W') => {
            if let Some(game) = browser.selected() {
                browser.open_workshop(game.id);
            }
        }
        KeyCode::Char('i') => browser.toggle_description(),
        KeyCode::Down | KeyCode::Char('j') => browser.next(),
        KeyCode::Up | KeyCode::Char('k') => browser.previous(),
        KeyCode::Char('g') => browser.home(),
        KeyCode::Char('G') => browser.end(),
        KeyCode::PageDown => browser.page_down(10),
        KeyCode::PageUp => browser.page_up(10),
        KeyCode::Char('\n') | KeyCode::Enter => {
            // Open the per-game Actions menu (command palette).
            // Power users keep the direct accelerators below;
            // the menu is the discoverable path to all of them
            // (Play is the default-highlighted row).
            browser.open_actions();
        }
        #[cfg(feature = "versioning")]
        KeyCode::Char('V') => {
            if let Some(game) = browser.selected() {
                browser.open_versions(game.id);
            }
        }
        #[cfg(feature = "collections")]
        KeyCode::Char('O') => {
            if let Some(game) = browser.selected() {
                browser.open_collections(game.id);
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('d') => {
            if let Some(game) = browser.selected() {
                shared::install(browser, client, &game)?;
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char(' ') => {
            if let Some(game) = browser.selected() {
                shared::pause_or_resume(browser, client, &game)?;
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('c') => {
            if let Some(game) = browser.selected() {
                shared::cancel_install(browser, &game);
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('x') => {
            // Only offer uninstall for installed games.
            if let Some(game) = browser.selected()
                && game.installed
            {
                browser.confirm_uninstall = true;
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('M') => {
            // Only offer move for installed games.
            if let Some(game) = browser.selected()
                && game.installed
            {
                browser.open_move(game.id);
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('K') => {
            // Only offer relink for installed games.
            if let Some(game) = browser.selected()
                && game.installed
            {
                browser.open_relink(game.id);
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('N') => {
            // Import registers an on-disk install, so it does
            // not require the game to already be installed.
            if let Some(game) = browser.selected() {
                browser.open_import(game.id);
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('v') => {
            if let Some(game) = browser.selected() {
                client.verify(&game)?;
            }
        }
        #[cfg(feature = "install")]
        KeyCode::Char('U') => {
            if let Some(game) = browser.selected() {
                client.update(&game)?;
            }
        }
        KeyCode::Char('D') => {
            if let Some(game) = browser.selected() {
                // Blocking fetch; failure leaves the overlay closed.
                let _ = browser.open_dlc(game.id);
            }
        }
        #[cfg(feature = "versioning")]
        KeyCode::Char('b') => {
            if let Some(game) = browser.selected() {
                // Blocking fetch; failure leaves the overlay closed.
                let _ = browser.open_branches(game.id);
            }
        }
        KeyCode::Char('P') => {
            // Blocking fetch; failure leaves the overlay closed.
            let _ = browser.open_proton();
        }
        KeyCode::Char('R') => browser.open_running(),
        KeyCode::Char('f') => {
            if let Some(game) = browser.selected() {
                shared::toggle_favourite(browser, config, &game)?;
            }
        }
        KeyCode::Char('H') => {
            if let Some(game) = browser.selected() {
                shared::hide(browser, config, &game)?;
            }
        }
        #[cfg(feature = "cloud")]
        KeyCode::Char('C') => {
            if let Some(game) = browser.selected() {
                browser.open_cloud(game.id);
            }
        }
        KeyCode::Char('o') => {
            if let Some(game) = browser.selected() {
                browser.open_depots(game.id);
            }
        }
        #[cfg(feature = "store")]
        KeyCode::Char('J') => browser.open_shop(),
        #[cfg(feature = "install")]
        KeyCode::Char('X') => browser.open_installs(),
        #[cfg(feature = "store")]
        KeyCode::Char('Y') => {
            if let Some(game) = browser.selected() {
                browser.open_store(game.id, &game.name);
            }
        }
        KeyCode::Char('E') => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::Steam, game.id);
            }
        }
        KeyCode::Char('B') => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::ProtonDb, game.id);
            }
        }
        KeyCode::Char('T') => {
            if let Some(game) = browser.selected() {
                browser.open_link(Link::ProtonIssues, game.id);
            }
        }
        KeyCode::Char('L') => {
            if let Some(game) = browser.selected() {
                browser.open_launch(game.id);
            }
        }
        _ => {}
    }
    Ok(Flow::Continue)
}
