//! Keys in the per-game Actions menu (the command palette): arrows move,
//! typing filters, and Enter runs the highlighted action the same way its
//! accelerator in the library does.

use ratatui::crossterm::event::KeyCode;

use super::shared;
use crate::app::AppResult;
use crate::app::library::Library;
use crate::browse::Action;

pub(super) fn key(lib: &mut Library, input: KeyCode) -> AppResult {
    let Library {
        browser,
        client,
        config,
        ..
    } = lib;
    match input {
        KeyCode::Esc => browser.close_actions(),
        KeyCode::Down => browser.actions_next(),
        KeyCode::Up => browser.actions_previous(),
        KeyCode::Backspace => browser.actions_filter_pop(),
        KeyCode::Char('\n') | KeyCode::Enter => {
            if let Some(action) = browser.selected_action() {
                browser.close_actions();
                if let Some(game) = browser.selected() {
                    match action {
                        Action::Play => {
                            if game.installed {
                                client.run(&game)?;
                            } else {
                                browser.confirm_install = true;
                            }
                        }
                        Action::PlayAs => browser.open_launch_targets(game.id),
                        Action::Install => shared::install(browser, client, &game)?,
                        Action::PauseResume => shared::pause_or_resume(browser, client, &game)?,
                        Action::CancelInstall => shared::cancel_install(browser, &game),
                        Action::Update => client.update(&game)?,
                        Action::Verify => client.verify(&game)?,
                        Action::Uninstall => {
                            if game.installed {
                                browser.confirm_uninstall = true;
                            }
                        }
                        Action::Versions => browser.open_versions(game.id),
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
                        Action::Workshop => browser.open_workshop(game.id),
                        Action::Cloud => browser.open_cloud(game.id),
                        Action::Achievements => browser.open_achievements(),
                        Action::Depots => browser.open_depots(game.id),
                        Action::Inventory => browser.open_inventory(game.id),
                        Action::Move => {
                            if game.installed {
                                browser.open_move(game.id);
                            }
                        }
                        Action::Relink => {
                            if game.installed {
                                browser.open_relink(game.id);
                            }
                        }
                        Action::Import => browser.open_import(game.id),
                        Action::Collections => browser.open_collections(game.id),
                        Action::Favourite => shared::toggle_favourite(browser, config, &game)?,
                        Action::Hide => shared::hide(browser, config, &game)?,
                    }
                }
            }
        }
        KeyCode::Char(c) => browser.actions_filter_push(c),
        _ => {}
    }
    Ok(())
}
