//! Keys in the per-game Actions menu (the command palette): arrows move,
//! typing filters, and Enter runs the highlighted action the same way its
//! accelerator in the library does.

use ratatui::crossterm::event::KeyCode;

use super::shared;
use crate::app::AppResult;
#[cfg(feature = "inventory")]
use crate::app::SensitiveView;
use crate::app::browser::Browser;
use crate::app::browser::{Action, Link};
use crate::app::library::Library;
use crate::keymap::{self, Key};

pub(super) fn key(lib: &mut Library, key: Key) -> AppResult {
    let input = key.code;
    let Library {
        #[cfg(feature = "inventory")]
        app,
        browser,
        client,
        config,
        ..
    } = lib;
    use crate::keymap::ActionsMenuAction as M;
    match (keymap::get().find::<M>(key, None), input) {
        (_, KeyCode::Esc) => browser.close_actions(),
        (Some(M::Next), _) | (None, KeyCode::Down) => browser.actions_next(),
        (Some(M::Previous), _) | (None, KeyCode::Up) => browser.actions_previous(),
        (None, KeyCode::Backspace) => browser.actions_filter_pop(),
        (Some(M::Run), _) => {
            if let Some(action) = browser.selected_action() {
                browser.close_actions();
                if let Some(game) = browser.selected() {
                    match action {
                        Action::Play => {
                            if game.installed {
                                client.run(&game)?;
                            } else {
                                offer_install(browser);
                            }
                        }
                        Action::PlayAs => browser.open_launch_targets(game.id),
                        #[cfg(feature = "install")]
                        Action::Install => shared::install(browser, client, &game)?,
                        #[cfg(feature = "install")]
                        Action::PauseResume => shared::pause_or_resume(browser, client, &game)?,
                        #[cfg(feature = "install")]
                        Action::CancelInstall => shared::cancel_install(browser, &game),
                        #[cfg(feature = "install")]
                        Action::Update => client.update(&game)?,
                        #[cfg(feature = "install")]
                        Action::Verify => client.verify(&game)?,
                        #[cfg(feature = "install")]
                        Action::Uninstall => {
                            if game.installed {
                                browser.confirm_uninstall = true;
                            }
                        }
                        #[cfg(feature = "versioning")]
                        Action::Versions => browser.open_versions(game.id),
                        #[cfg(feature = "versioning")]
                        Action::Branches => {
                            let _ = browser.open_branches(game.id);
                        }
                        Action::GameSettings => browser.open_game_config(game.id),
                        Action::Proton => {
                            let _ = browser.open_proton();
                        }
                        Action::LaunchOptions => browser.open_launch(game.id),
                        Action::Dlc => {
                            let _ = browser.open_dlc(game.id);
                        }
                        #[cfg(feature = "workshop")]
                        Action::Workshop => browser.open_workshop(game.id),
                        #[cfg(feature = "cloud")]
                        Action::Cloud => browser.open_cloud(game.id),
                        Action::Achievements => browser.open_achievements(),
                        Action::Depots => browser.open_depots(game.id),
                        #[cfg(feature = "inventory")]
                        Action::Inventory => {
                            app.open_sensitive(browser, SensitiveView::Inventory(game.id))
                        }
                        #[cfg(feature = "install")]
                        Action::Move => {
                            if game.installed {
                                browser.open_move(game.id);
                            }
                        }
                        #[cfg(feature = "install")]
                        Action::Relink => {
                            if game.installed {
                                browser.open_relink(game.id);
                            }
                        }
                        #[cfg(feature = "install")]
                        Action::Import => browser.open_import(game.id),
                        #[cfg(feature = "collections")]
                        Action::Collections => browser.open_collections(game.id),
                        #[cfg(feature = "store")]
                        Action::Store => browser.open_store(game.id, &game.name),
                        Action::GameInfo => browser.open_game_info(game.id, &game.name),
                        Action::SteamPage => browser.open_link(Link::Steam, game.id),
                        Action::ProtonDb => browser.open_link(Link::ProtonDb, game.id),
                        Action::ProtonIssues => browser.open_link(Link::ProtonIssues, game.id),
                        Action::SteamDb => browser.open_link(Link::SteamDb, game.id),
                        Action::Favourite => shared::toggle_favourite(browser, config, &game)?,
                        Action::Hide => shared::hide(browser, config, &game)?,
                    }
                }
            }
        }
        (None, KeyCode::Char(c)) if !key.ctrl && !key.alt => browser.actions_filter_push(c),
        _ => {}
    }
    Ok(())
}

/// Play on a game that isn't installed: ask to install it, or, in a build
/// without installs, say why it can't start.
fn offer_install(browser: &mut Browser) {
    #[cfg(feature = "install")]
    {
        browser.confirm_install = true;
    }
    #[cfg(not(feature = "install"))]
    {
        browser.notice = Some("Not installed (this build has no installs).".to_string());
    }
}
