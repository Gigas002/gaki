//! Keys in the library's overlays and prompts. The first open one, in the
//! order below, takes the key.

use ratatui::crossterm::event::KeyCode;

use super::shared;
use crate::app::library::Library;
use crate::app::{AppResult, Mode};
use crate::interface::aurelia;

/// Handle `input` in the open overlay. Returns `false` when none is open.
pub(super) fn key(lib: &mut Library, input: KeyCode) -> AppResult<bool> {
    let Library {
        app,
        browser,
        client,
        cached,
        ..
    } = lib;
    if browser.show_versions {
        // Versions & pinning overlay. When prompting for a
        // downgrade manifest id, keys edit that input instead.
        if browser.versions_input.is_some() {
            match input {
                KeyCode::Esc => browser.cancel_downgrade_input(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.start_downgrade(),
                KeyCode::Backspace => browser.versions_input_pop(),
                KeyCode::Char(c) => browser.versions_input_push(c),
                _ => {}
            }
        } else {
            match input {
                KeyCode::Esc | KeyCode::Char('q') => browser.close_versions(),
                KeyCode::Down | KeyCode::Char('j') => browser.versions_next(),
                KeyCode::Up | KeyCode::Char('k') => browser.versions_previous(),
                KeyCode::Char('p') => browser.pin_current(),
                KeyCode::Char('u') => browser.unpin_current(),
                KeyCode::Char('d') => browser.begin_downgrade_input(),
                _ => {}
            }
        }
    } else if browser.show_launch_targets {
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_launch_targets(),
            KeyCode::Down | KeyCode::Char('j') => browser.launch_targets_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.launch_targets_previous(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                if let Some(opts) = browser.selected_launch_target() {
                    browser.close_launch_targets();
                    if let Some(game) = browser.selected() {
                        client.run_with(&game, opts)?;
                    }
                }
            }
            _ => {}
        }
    } else if browser.show_game_config {
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_game_config(),
            KeyCode::Down | KeyCode::Char('j') => browser.game_config_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.game_config_previous(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.game_config_activate(),
            KeyCode::Char('x') => browser.game_config_remove_script(),
            _ => {}
        }
    } else if browser.show_collections {
        // Collections overlay. In create mode, keys edit the name.
        if browser.collections_input.is_some() {
            match input {
                KeyCode::Esc => browser.cancel_collection_create(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.commit_collection_create(),
                KeyCode::Backspace => browser.collections_input_pop(),
                KeyCode::Char(c) => browser.collections_input_push(c),
                _ => {}
            }
        } else {
            match input {
                KeyCode::Esc | KeyCode::Char('q') => browser.close_collections(),
                KeyCode::Down | KeyCode::Char('j') => browser.collections_next(),
                KeyCode::Up | KeyCode::Char('k') => browser.collections_previous(),
                KeyCode::Char('a') => browser.add_game_to_selected_collection(),
                KeyCode::Char('r') => browser.remove_game_from_selected_collection(),
                KeyCode::Char('n') => browser.begin_collection_create(),
                KeyCode::Char('x') => browser.delete_selected_collection(),
                KeyCode::Char('P') => browser.collections_pull(),
                KeyCode::Char('U') => browser.collections_push(),
                KeyCode::Char('S') => browser.collections_sync(),
                _ => {}
            }
        }
    } else if browser.show_cloud {
        // Steam Cloud overlay: Esc/q close, s syncs both ways,
        // d downloads only, u uploads only; each re-fetches.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_cloud(),
            KeyCode::Char('s') => {
                if let Some(game) = browser.selected() {
                    browser.sync_cloud(game.id, aurelia::CloudDirection::Both);
                }
            }
            KeyCode::Char('d') => {
                if let Some(game) = browser.selected() {
                    browser.sync_cloud(game.id, aurelia::CloudDirection::Down);
                }
            }
            KeyCode::Char('u') => {
                if let Some(game) = browser.selected() {
                    browser.sync_cloud(game.id, aurelia::CloudDirection::Up);
                }
            }
            _ => {}
        }
    } else if browser.show_achievements {
        // Achievements overlay: Esc/q close, j/k scroll.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_achievements(),
            KeyCode::Down | KeyCode::Char('j') => browser.ach_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.ach_scroll_up(),
            _ => {}
        }
    } else if browser.show_chat {
        // Chat view: Esc close, Enter send, typing composes.
        match input {
            KeyCode::Esc => browser.close_chat(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.chat_send(),
            KeyCode::Backspace => browser.chat_pop(),
            KeyCode::Char(c) => browser.chat_push(c),
            _ => {}
        }
    } else if browser.show_friend_add {
        // Add-friend prompt: type the reference, Enter to resolve
        // it via `friends search`, a to send the request via
        // `friends add` (kept open to show status), Esc to cancel.
        match input {
            KeyCode::Esc => browser.close_friend_add(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.friend_search(),
            KeyCode::Char('a') => browser.friend_add_confirm(),
            KeyCode::Backspace => browser.friend_add_pop(),
            KeyCode::Char(c) => browser.friend_add_push(c),
            _ => {}
        }
    } else if browser.confirm_friend_remove {
        // Remove-friend confirmation: y confirms, anything else cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                browser.friend_remove_confirm();
                browser.confirm_friend_remove = false;
            }
            _ => browser.confirm_friend_remove = false,
        }
    } else if browser.friends_focused() {
        // Friends panel focused: j/k move the highlight (the whole
        // list scrolls under it), c/Enter open the in-app chat, t
        // pops the chat into a new terminal window, a opens the
        // add-friend prompt, x removes the highlighted friend
        // (confirmed), Esc/F/q unfocus.
        match input {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('F') => browser.unfocus_friends(),
            KeyCode::Enter | KeyCode::Char('c') => browser.open_chat(),
            KeyCode::Char('t') => browser.open_chat_terminal(),
            KeyCode::Char('a') => browser.open_friend_add(),
            KeyCode::Char('x') => {
                if browser.selected_friend().is_some() {
                    browser.confirm_friend_remove = true;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => browser.friends_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.friends_scroll_up(),
            // Tab/Shift-Tab keep cycling the tab ring so the
            // Friends tab is not a dead end — stepping right off
            // it wraps back to All.
            KeyCode::Tab => browser.cycle_filter(true),
            KeyCode::BackTab => browser.cycle_filter(false),
            _ => {}
        }
    } else if browser.show_inventory {
        // Inventory overlay: Esc/q close, j/k scroll.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_inventory(),
            KeyCode::Down | KeyCode::Char('j') => browser.inv_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.inv_scroll_up(),
            _ => {}
        }
    } else if browser.show_launch {
        // Launch-options overlay: Esc/q close, j/k scroll.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_launch(),
            KeyCode::Down | KeyCode::Char('j') => browser.launch_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.launch_scroll_up(),
            _ => {}
        }
    } else if browser.show_market {
        // Market listings overlay: Esc/q close, j/k scroll.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_market(),
            KeyCode::Down | KeyCode::Char('j') => browser.market_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.market_scroll_up(),
            _ => {}
        }
    } else if browser.show_market_search {
        // Market search overlay: typing edits the query, Enter
        // runs the search (off-thread), Up/Down move the result
        // highlight, Tab prices the highlighted result (off-thread),
        // Esc closes. q/j/k are NOT shortcuts here so they can be
        // typed into the query.
        match input {
            KeyCode::Esc => browser.close_market_search(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.submit_market_search(),
            KeyCode::Down => browser.market_result_next(),
            KeyCode::Up => browser.market_result_previous(),
            KeyCode::Backspace => browser.market_query_pop(),
            // Tab prices the highlighted result. A non-character
            // key is used so every letter stays typeable into the
            // query (the help overlay documents this).
            KeyCode::Tab => browser.lookup_market_price(),
            KeyCode::Char(c) => browser.market_query_push(c),
            _ => {}
        }
    } else if browser.show_workshop {
        if browser.workshop_comments_open {
            // Comments sub-pane (over the browse results): Esc
            // returns to the results, Up/Down scroll. No text is
            // edited here, so j/k are also accepted as scroll keys.
            match input {
                KeyCode::Esc => browser.close_workshop_comments(),
                KeyCode::Down | KeyCode::Char('j') => browser.workshop_comments_scroll_down(),
                KeyCode::Up | KeyCode::Char('k') => browser.workshop_comments_scroll_up(),
                _ => {}
            }
        } else if browser.workshop_browse {
            // Browse/search pane: the query line takes typed
            // text (Enter searches off-thread). Navigation/action
            // use non-text keys so they never collide with what
            // the user is typing: Up/Down move the highlight,
            // Tab subscribes/unsubscribes it off-thread, F1/F2
            // rate it up/down off-thread, F3 opens its comments
            // sub-pane, and Esc returns to the subscribed list.
            match input {
                KeyCode::Esc => browser.workshop_exit_browse(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.start_workshop_search(),
                KeyCode::Down => browser.workshop_next(),
                KeyCode::Up => browser.workshop_previous(),
                KeyCode::Tab => browser.workshop_toggle_subscribe_selected(),
                KeyCode::F(1) => browser.workshop_rate_selected(true),
                KeyCode::F(2) => browser.workshop_rate_selected(false),
                KeyCode::F(3) => browser.workshop_open_comments(),
                KeyCode::Backspace => browser.workshop_pop_query(),
                KeyCode::Char(c) => browser.workshop_push_query(c),
                _ => {}
            }
        } else {
            // Subscribed-items list: Esc/q close, j/k scroll,
            // 'b' opens the browse/search pane.
            match input {
                KeyCode::Esc | KeyCode::Char('q') => browser.close_workshop(),
                KeyCode::Char('b') => browser.workshop_enter_browse(),
                KeyCode::Down | KeyCode::Char('j') => browser.workshop_scroll_down(),
                KeyCode::Up | KeyCode::Char('k') => browser.workshop_scroll_up(),
                _ => {}
            }
        }
    } else if browser.confirm_install {
        // "Not installed — install now?" prompt: y opens the
        // install-location picker for the game, anything cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                browser.confirm_install = false;
                if let Some(game) = browser.selected() {
                    shared::install(browser, client, &game)?;
                }
            }
            _ => browser.confirm_install = false,
        }
    } else if browser.show_install_picker {
        // Install-location picker: Up/Down choose a library,
        // Enter installs the selected game into it, Esc cancels.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_install_picker(),
            KeyCode::Down | KeyCode::Char('j') => browser.install_picker_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.install_picker_previous(),
            KeyCode::Enter | KeyCode::Char('\n') => {
                if !browser.selected_library_fits() {
                    // Not enough room — keep the picker open so
                    // another drive can be chosen.
                    browser.notice =
                        Some("Not enough space on that drive — pick another.".to_string());
                } else {
                    if let (Some(game), Some(library)) =
                        (browser.selected(), browser.selected_install_library())
                    {
                        let control = browser.begin_install(game.id, Some(library.clone()));
                        client.install(&game, control, Some(library))?;
                    }
                    browser.close_install_picker();
                }
            }
            _ => {}
        }
    } else if browser.confirm_uninstall {
        // Uninstall confirmation prompt: y confirms, anything else cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(game) = browser.selected() {
                    match aurelia::uninstall(game.id) {
                        Ok(()) => {
                            // Reload the library so the listing
                            // reflects the now-uninstalled game
                            // (mirrors 'r').
                            shared::reload(app, client, cached)?;
                        }
                        // A failed uninstall must not crash the
                        // TUI — surface it and stay running.
                        Err(err) => {
                            browser.notice = Some(format!("Uninstall failed: {err}"));
                        }
                    }
                }
                browser.confirm_uninstall = false;
            }
            _ => browser.confirm_uninstall = false,
        }
    } else if browser.show_dlc {
        // DLC overlay: navigate and toggle the highlighted DLC.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_dlc(),
            KeyCode::Down | KeyCode::Char('j') => browser.dlc_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.dlc_previous(),
            KeyCode::Char('e') => {
                if let Some(entry) = browser.selected_dlc() {
                    let _ = aurelia::set_dlc(entry.app_id as i32, true);
                    let _ = browser.refresh_dlc();
                }
            }
            KeyCode::Char('x') => {
                if let Some(entry) = browser.selected_dlc() {
                    let _ = aurelia::set_dlc(entry.app_id as i32, false);
                    let _ = browser.refresh_dlc();
                }
            }
            _ => {}
        }
    } else if browser.show_branches {
        // Branches overlay: navigate and switch to the highlighted branch.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_branches(),
            KeyCode::Down | KeyCode::Char('j') => browser.branch_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.branch_previous(),
            KeyCode::Char('\n') | KeyCode::Enter | KeyCode::Char('s') => {
                if let Some(branch) = browser.selected_branch() {
                    let _ = aurelia::set_branch(browser.branch_app_id(), &branch.name);
                    browser.close_branches();
                }
            }
            _ => {}
        }
    } else if browser.show_running {
        // Running overlay: navigate and stop the highlighted game.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_running(),
            KeyCode::Down | KeyCode::Char('j') => browser.running_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.running_previous(),
            KeyCode::Char('s') | KeyCode::Char('x') => {
                if let Some(r) = browser.selected_running() {
                    let _ = aurelia::stop(r.app_id as i32);
                    browser.refresh_running();
                }
            }
            _ => {}
        }
    } else if browser.show_proton && browser.confirm_proton_uninstall {
        // Proton uninstall confirmation: y confirms (remove the
        // custom runtime + refresh), anything else cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = browser.do_proton_uninstall();
                browser.confirm_proton_uninstall = false;
            }
            _ => browser.confirm_proton_uninstall = false,
        }
    } else if browser.show_proton {
        // Proton overlay: navigate, set the highlighted runtime as
        // the global default, install it, or uninstall a custom one.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_proton(),
            KeyCode::Down | KeyCode::Char('j') => browser.proton_next(),
            KeyCode::Up | KeyCode::Char('k') => browser.proton_previous(),
            KeyCode::Char('d') => {
                if let Some(p) = browser.selected_proton() {
                    let _ = aurelia::proton_default(&p.name);
                    // Re-fetch so the [default] marker updates,
                    // keeping the current row highlighted.
                    let keep = browser.proton_index;
                    if browser.open_proton().is_ok() {
                        browser.proton_index =
                            keep.min(browser.protons.len().saturating_sub(1));
                    }
                }
            }
            KeyCode::Char('i') => {
                // Queue a (long-running) download off the UI thread;
                // progress streams into the proton status cell.
                if let Some(p) = browser.selected_proton()
                    && !p.installed {
                        browser.clear_proton_status();
                        let _ = client
                            .proton_install(&p.name, browser.proton_status.clone());
                    }
            }
            KeyCode::Char('u')
                // Only installed official runtimes are removable.
                if browser.selected_proton_uninstallable() => {
                    browser.confirm_proton_uninstall = true;
                }
            _ => {}
        }
    } else if browser.show_depots {
        // Depots overlay: Esc/q close, j/k scroll.
        match input {
            KeyCode::Esc | KeyCode::Char('q') => browser.close_depots(),
            KeyCode::Down | KeyCode::Char('j') => browser.depots_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.depots_scroll_up(),
            _ => {}
        }
    } else if browser.show_move {
        // Move prompt: type the destination library path, Enter
        // to relocate (kept open to show status), Esc to cancel.
        match input {
            KeyCode::Esc => browser.close_move(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                let _ = browser.do_move();
            }
            KeyCode::Backspace => browser.move_pop(),
            KeyCode::Char(c) => browser.move_push(c),
            _ => {}
        }
    } else if browser.show_relink {
        // Relink prompt: type the destination library path, Enter
        // to relink (kept open to show status), Esc to cancel.
        match input {
            KeyCode::Esc => browser.close_relink(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                let _ = browser.do_relink();
            }
            KeyCode::Backspace => browser.relink_pop(),
            KeyCode::Char(c) => browser.relink_push(c),
            _ => {}
        }
    } else if browser.show_import {
        // Import prompt: type the library path holding the existing
        // files, Enter to register (kept open to show status), Esc
        // to cancel.
        match input {
            KeyCode::Esc => browser.close_import(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                let _ = browser.do_import();
            }
            KeyCode::Backspace => browser.import_pop(),
            KeyCode::Char(c) => browser.import_push(c),
            _ => {}
        }
    } else if browser.show_help {
        // Help overlay: Esc/q/? close, j/k scroll.
        match input {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                browser.show_help = false;
            }
            KeyCode::Down | KeyCode::Char('j') => browser.help_scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => browser.help_scroll_up(),
            _ => {}
        }
    } else if browser.show_account {
        // `o` logs out of Steam; any other key dismisses the overlay.
        match input {
            KeyCode::Char('o') => {
                let _ = aurelia::logout();
                browser.show_account = false;
                app.mode = Mode::Login;
            }
            _ => browser.show_account = false,
        }
    } else if browser.show_config {
        // Settings overlay. While editing the proxy URL, keys edit
        // that input; otherwise dismiss / toggle presence / edit or
        // clear the proxy.
        if browser.config_proxy_input.is_some() {
            match input {
                KeyCode::Esc => browser.cancel_proxy_edit(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.commit_proxy_edit(),
                KeyCode::Backspace => browser.proxy_input_pop(),
                KeyCode::Char(c) => browser.proxy_input_push(c),
                _ => {}
            }
        } else {
            match input {
                KeyCode::Esc | KeyCode::Char('q') => browser.close_config(),
                KeyCode::Char('o') => {
                    // Flip whatever the config currently shows
                    // (default offline when unknown), then re-fetch.
                    let online = browser
                        .config_info
                        .as_ref()
                        .map(|c| c.is_online())
                        .unwrap_or(false);
                    let _ = aurelia::set_presence(!online);
                    let _ = browser.open_config();
                }
                KeyCode::Char('e') => browser.begin_proxy_edit(),
                KeyCode::Char('c') => browser.clear_proxy(),
                _ => {}
            }
        }
    } else if browser.show_wallet {
        // Any key dismisses the wallet overlay.
        browser.show_wallet = false;
    } else if browser.filtering {
        // Live text filter focused: typing edits the query.
        match input {
            KeyCode::Esc => {
                browser.filtering = false;
                browser.clear_query();
            }
            KeyCode::Char('\n') | KeyCode::Enter => browser.filtering = false,
            KeyCode::Backspace => browser.pop_query(),
            KeyCode::Down => browser.next(),
            KeyCode::Up => browser.previous(),
            KeyCode::Char(c) => browser.push_query(c),
            _ => {}
        }
    } else {
        return Ok(false);
    }
    Ok(true)
}
