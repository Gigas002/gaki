//! Game actions shared by the Actions menu, the library's accelerators and
//! the prompts, so each one behaves the same from every place it's reached.

use crate::app::browser::{Browser, InstallPhase};
use crate::app::client::Client;
use crate::app::game::Game;
use crate::app::{App, AppResult, Mode};
use crate::config::Config;

/// Install `game`. The location picker opens first; when no libraries can
/// be listed, it installs into the default one.
pub(super) fn install(browser: &mut Browser, client: &Client, game: &Game) -> AppResult {
    if !browser.open_install_picker(game.id) {
        let control = browser.begin_install(game.id, None);
        client.install(game, control, None)?;
    }
    Ok(())
}

/// Pause an in-flight install, or resume a paused one into the library it
/// started in. The download stays on disk, so it picks up where it stopped.
pub(super) fn pause_or_resume(browser: &mut Browser, client: &Client, game: &Game) -> AppResult {
    match browser.install_phase(game) {
        InstallPhase::Active => browser.pause_install(game.id),
        InstallPhase::Paused => {
            let library = browser.install_library_for(game.id);
            let control = browser.begin_install(game.id, library.clone());
            client.install(game, control, library)?;
        }
        InstallPhase::Idle => {}
    }
    Ok(())
}

/// Cancel an in-flight or paused install.
pub(super) fn cancel_install(browser: &mut Browser, game: &Game) {
    if browser.install_phase(game) != InstallPhase::Idle {
        browser.stop_install(game.id);
    }
}

/// Add `game` to the favourites, or take it out.
pub(super) fn toggle_favourite(
    browser: &mut Browser,
    config: &mut Config,
    game: &Game,
) -> AppResult {
    if config.favorite_games.contains(&game.id) {
        config.favorite_games.retain(|&x| x != game.id);
    } else {
        config.favorite_games.push(game.id);
    }
    config.save()?;
    browser.refresh();
    Ok(())
}

/// Hide `game` from the library.
pub(super) fn hide(browser: &mut Browser, config: &mut Config, game: &Game) -> AppResult {
    config.hidden_games.push(game.id);
    config.save()?;
    browser.refresh();
    Ok(())
}

/// Reload the library from Steam, skipping the cache.
pub(super) fn reload(app: &mut App, client: &Client, cached: &mut bool) -> AppResult {
    *cached = false;
    app.mode = Mode::Loading;
    client.restart()?;
    Ok(())
}
