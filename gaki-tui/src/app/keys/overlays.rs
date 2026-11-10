//! Keys in the library's overlays and prompts. The first open one, in the
//! order below, takes the key. Overlays with a `keybinds.toml` section look
//! their keys up in [`keymap`]: their own section first, then `[overlay]`.
//! Text prompts and y/n confirms keep fixed keys.

use ratatui::crossterm::event::KeyCode;

use crate::keymap::{self, Actions, Key, OverlayAction as O};

#[cfg(feature = "install")]
use super::shared;
use crate::app::library::Library;
use crate::app::steam;
use crate::app::{AppResult, Mode};

/// Handle `input` in the open overlay; overlays a build leaves out are
/// skipped. Returns `false` when none is open.
pub(super) fn key(lib: &mut Library, key: Key) -> AppResult<bool> {
    let input = key.code;
    let Library {
        app,
        browser,
        client,
        ..
    } = lib;
    #[cfg(feature = "versioning")]
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
            use crate::keymap::VersionsAction as V;
            match own::<V>(key, None) {
                _ if input == KeyCode::Esc => browser.close_versions(),
                Some(V::Pin) => browser.pin_current(),
                Some(V::Unpin) => browser.unpin_current(),
                Some(V::Downgrade) => browser.begin_downgrade_input(),
                Some(V::SteamDb) => {
                    if let Some(m) = browser.selected_manifest() {
                        let id = m.depot_id as i32;
                        browser.open_link(crate::app::browser::Link::SteamDbDepot, id);
                    }
                }
                None => match common(key) {
                    Some(O::Close) => browser.close_versions(),
                    Some(O::Down) => browser.versions_next(),
                    Some(O::Up) => browser.versions_previous(),
                    _ => {}
                },
            }
        }
        return Ok(true);
    }
    if browser.show_launch_targets {
        use crate::keymap::LaunchTargetsAction as Lt;
        match own::<Lt>(key, None) {
            _ if input == KeyCode::Esc => browser.close_launch_targets(),
            Some(Lt::Launch) => {
                if let Some(opts) = browser.selected_launch_target() {
                    browser.close_launch_targets();
                    if let Some(game) = browser.selected() {
                        client.run_with(&game, opts)?;
                    }
                }
            }
            None => match common(key) {
                Some(O::Close) => browser.close_launch_targets(),
                Some(O::Down) => browser.launch_targets_next(),
                Some(O::Up) => browser.launch_targets_previous(),
                _ => {}
            },
        }
        return Ok(true);
    }
    if browser.show_game_config {
        // Typing a text row (Args, Env, Shader cache): keys edit the text.
        if browser.game_config_input.is_some() {
            match input {
                KeyCode::Esc => browser.game_config_input_cancel(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.game_config_input_submit(),
                KeyCode::Backspace => browser.game_config_input_pop(),
                KeyCode::Char(c) => browser.game_config_input_push(c),
                _ => {}
            }
            return Ok(true);
        }
        use crate::keymap::GameSettingsAction as Gs;
        match own::<Gs>(key, None) {
            _ if input == KeyCode::Esc => browser.close_game_config(),
            Some(Gs::Edit) => browser.game_config_activate(),
            None => match common(key) {
                Some(O::Close) => browser.close_game_config(),
                Some(O::Down) => browser.game_config_next(),
                Some(O::Up) => browser.game_config_previous(),
                _ => {}
            },
        }
        return Ok(true);
    }
    #[cfg(feature = "collections")]
    if browser.show_collections {
        // Collections overlay. In the create/rename prompt, keys edit
        // the name; the delete confirm takes y/n.
        if browser.collections_delete.is_some() {
            browser.answer_collection_delete(matches!(input, KeyCode::Char('y' | 'Y')));
        } else if browser.collections_input.is_some() {
            match input {
                KeyCode::Esc => browser.cancel_collection_input(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.commit_collection_input(),
                KeyCode::Backspace => browser.collections_input_pop(),
                KeyCode::Char(c) => browser.collections_input_push(c),
                _ => {}
            }
        } else if input == KeyCode::Esc {
            browser.close_collections();
        } else {
            use crate::keymap::CollectionsAction as C;
            match own::<C>(key, None) {
                Some(C::Add) => browser.add_game_to_selected_collection(),
                Some(C::Remove) => browser.remove_game_from_selected_collection(),
                Some(C::New) => browser.begin_collection_create(),
                Some(C::Rename) => browser.begin_collection_rename(),
                Some(C::Delete) => browser.begin_collection_delete(),
                Some(C::Pull) => browser.collections_pull(),
                Some(C::Push) => browser.collections_push(),
                Some(C::Sync) => browser.collections_sync(),
                None => match common(key) {
                    Some(O::Close) => browser.close_collections(),
                    Some(O::Down) => browser.collections_next(),
                    Some(O::Up) => browser.collections_previous(),
                    _ => {}
                },
            }
        }
        return Ok(true);
    }
    #[cfg(feature = "cloud")]
    if browser.show_cloud {
        // Steam Cloud overlay: Esc/q close, s syncs both ways,
        // d downloads only, u uploads only (each in the background);
        // after conflicts, c keeps the cloud copies and l the local
        // ones, both after a y/n.
        if browser.cloud_state.confirm.is_some() {
            browser.answer_cloud_resolve(matches!(input, KeyCode::Char('y' | 'Y')));
            return Ok(true);
        }
        if input == KeyCode::Esc {
            browser.close_cloud();
            return Ok(true);
        }
        use crate::keymap::CloudAction as C;
        match own::<C>(key, None) {
            Some(C::Sync) => browser.sync_cloud(steam::CloudDirection::Both),
            Some(C::Download) => browser.sync_cloud(steam::CloudDirection::Down),
            Some(C::Upload) => browser.sync_cloud(steam::CloudDirection::Up),
            Some(C::KeepCloud) => browser.begin_cloud_resolve(steam::CloudResolve::KeepCloud),
            Some(C::KeepLocal) => browser.begin_cloud_resolve(steam::CloudResolve::KeepLocal),
            None if common(key) == Some(O::Close) => browser.close_cloud(),
            None => {}
        }
        return Ok(true);
    }
    if browser.show_achievements {
        // Achievements overlay: Esc/q close, j/k scroll.
        match common(key) {
            _ if input == KeyCode::Esc => browser.close_achievements(),
            Some(O::Close) => browser.close_achievements(),
            Some(O::Down) => browser.ach_scroll_down(),
            Some(O::Up) => browser.ach_scroll_up(),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "social")]
    if browser.show_chat {
        // Chat view: Esc close, Enter send, typing composes.
        match input {
            KeyCode::Esc => browser.close_chat(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.chat_send(),
            KeyCode::Backspace => browser.chat_pop(),
            KeyCode::Char(c) => browser.chat_push(c),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "social")]
    if browser.user_profile.is_some() {
        // User profile overlay: the lookup prompt takes text; otherwise
        // `/` looks someone else up, E opens the page, j/k scroll.
        if browser.profile_input_open() {
            match input {
                KeyCode::Esc => browser.cancel_profile_input(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.commit_profile_input(),
                KeyCode::Backspace => browser.profile_input_pop(),
                KeyCode::Char(c) => browser.profile_input_push(c),
                _ => {}
            }
        } else {
            use crate::keymap::UserProfileAction as Up;
            match own::<Up>(key, None) {
                _ if input == KeyCode::Esc => browser.close_user_profile(),
                Some(Up::Lookup) => browser.begin_profile_lookup(),
                Some(Up::ProfilePage) => browser.open_profile_page(),
                None => match common(key) {
                    Some(O::Close) => browser.close_user_profile(),
                    Some(O::Down) => browser.profile_scroll(1),
                    Some(O::Up) => browser.profile_scroll(-1),
                    Some(O::PageDown) => browser.profile_scroll(10),
                    Some(O::PageUp) => browser.profile_scroll(-10),
                    _ => {}
                },
            }
        }
        return Ok(true);
    }
    #[cfg(feature = "social")]
    if browser.show_friend_add {
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
        return Ok(true);
    }
    #[cfg(feature = "social")]
    if browser.confirm_friend_remove {
        // Remove-friend confirmation: y confirms, anything else cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                browser.friend_remove_confirm();
                browser.confirm_friend_remove = false;
            }
            _ => browser.confirm_friend_remove = false,
        }
        return Ok(true);
    }
    // The Friends panel stays focused under a friend's wishlist (`w`), so
    // the Store overlays above it take keys first.
    #[cfg(feature = "social")]
    if browser.friends_focused() && !store_overlay_open(browser) {
        // Friends panel focused: j/k move the highlight (the whole
        // list scrolls under it), c/Enter open the in-app chat, t
        // pops the chat into a new terminal window, a opens the
        // add-friend prompt, v shows the highlighted friend's profile,
        // / looks a user up, x removes the highlighted friend
        // (confirmed), w shows their wishlist in the Steam Store
        // overlay, Esc/F/q unfocus. `[friends]` rebinds them.
        use crate::keymap::{FriendsAction as Fr, LibraryAction};
        match own::<Fr>(key, None) {
            _ if input == KeyCode::Esc => browser.unfocus_friends(),
            Some(Fr::Leave) => browser.unfocus_friends(),
            Some(Fr::Chat) => browser.open_chat(),
            Some(Fr::ChatWindow) => browser.open_chat_terminal(),
            Some(Fr::Add) => browser.open_friend_add(),
            Some(Fr::Profile) => browser.open_friend_profile(),
            Some(Fr::Lookup) => browser.open_profile_lookup(),
            Some(Fr::Remove) => {
                if browser.selected_friend().is_some() {
                    browser.confirm_friend_remove = true;
                }
            }
            #[cfg(feature = "store")]
            Some(Fr::Wishlist) => browser.open_friend_wishlist(),
            #[cfg(not(feature = "store"))]
            Some(Fr::Wishlist) => {}
            None => match common(key) {
                Some(O::Close) => browser.unfocus_friends(),
                Some(O::Down) => browser.friends_scroll_down(),
                Some(O::Up) => browser.friends_scroll_up(),
                // The library's tab keys keep cycling the tab ring, so
                // the Friends tab is not a dead end — stepping right
                // off it wraps back to All.
                _ => match keymap::get().library_action(key) {
                    Some(LibraryAction::NextTab) => browser.cycle_filter(true),
                    Some(LibraryAction::PreviousTab) => browser.cycle_filter(false),
                    _ => {}
                },
            },
        }
        return Ok(true);
    }
    #[cfg(feature = "inventory")]
    if browser.show_inventory {
        // Inventory overlay: Esc/q close, j/k scroll.
        match common(key) {
            _ if input == KeyCode::Esc => browser.close_inventory(),
            Some(O::Close) => browser.close_inventory(),
            Some(O::Down) => browser.inv_scroll_down(),
            Some(O::Up) => browser.inv_scroll_up(),
            _ => {}
        }
        return Ok(true);
    }
    if browser.show_launch {
        // Launch-options overlay: Esc/q close, j/k scroll.
        match common(key) {
            _ if input == KeyCode::Esc => browser.close_launch(),
            Some(O::Close) => browser.close_launch(),
            Some(O::Down) => browser.launch_scroll_down(),
            Some(O::Up) => browser.launch_scroll_up(),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "market")]
    if browser.show_market {
        // Market listings overlay: Esc/q close, j/k scroll.
        match common(key) {
            _ if input == KeyCode::Esc => browser.close_market(),
            Some(O::Close) => browser.close_market(),
            Some(O::Down) => browser.market_scroll_down(),
            Some(O::Up) => browser.market_scroll_up(),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "market")]
    if browser.show_market_search {
        // Market search overlay: typing edits the query, Enter
        // runs the search (off-thread), Up/Down move the result
        // highlight, Tab prices the highlighted result (off-thread),
        // Esc closes. q/j/k are NOT shortcuts here so they can be
        // typed into the query.
        // Pricing uses a key that doesn't type (Tab by default), so
        // every letter stays typeable into the query.
        use crate::keymap::MarketSearchAction as Ms;
        match (own::<Ms>(key, None), input) {
            (Some(Ms::Price), _) => browser.lookup_market_price(),
            (None, KeyCode::Esc) => browser.close_market_search(),
            (None, KeyCode::Enter) => browser.submit_market_search(),
            (None, KeyCode::Down) => browser.market_result_next(),
            (None, KeyCode::Up) => browser.market_result_previous(),
            (None, KeyCode::Backspace) => browser.market_query_pop(),
            (None, KeyCode::Char(c)) if !key.ctrl && !key.alt => browser.market_query_push(c),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "workshop")]
    if browser.show_workshop {
        if browser.workshop_comments_open {
            workshop_comments_key(browser, key);
        } else if browser.workshop_browse {
            // Browse/search pane: the query line takes typed
            // text (Enter searches off-thread). Navigation/action
            // use non-text keys so they never collide with what
            // the user is typing: Up/Down move the highlight,
            // Tab subscribes/unsubscribes it off-thread, F1/F2
            // rate it up/down off-thread, F3 opens its comments
            // sub-pane, F4 installs it, and Esc returns to the
            // status list.
            use crate::keymap::WorkshopBrowseAction as B;
            match (own::<B>(key, None), input) {
                (Some(B::Subscribe), _) => browser.workshop_toggle_subscribe_selected(),
                (Some(B::RateUp), _) => browser.workshop_rate_selected(true),
                (Some(B::RateDown), _) => browser.workshop_rate_selected(false),
                (Some(B::Comments), _) => browser.workshop_open_comments(),
                (Some(B::Install), _) => browser.workshop_install_result(),
                (None, KeyCode::Esc) => browser.workshop_exit_browse(),
                (None, KeyCode::Enter) => browser.start_workshop_search(),
                (None, KeyCode::Down) => browser.workshop_next(),
                (None, KeyCode::Up) => browser.workshop_previous(),
                (None, KeyCode::Backspace) => browser.workshop_pop_query(),
                (None, KeyCode::Char(c)) if !key.ctrl && !key.alt => browser.workshop_push_query(c),
                _ => {}
            }
        } else if browser.workshop_confirm_open() {
            browser.answer_workshop_uninstall(matches!(input, KeyCode::Char('y' | 'Y')));
        } else {
            // Status list: Esc/q close, j/k move, b browse, i
            // install/update, u uninstall (asks), U update all
            // outdated, r reload.
            use crate::keymap::WorkshopAction as W;
            match own::<W>(key, None) {
                _ if input == KeyCode::Esc => browser.close_workshop(),
                Some(W::Browse) => browser.workshop_enter_browse(),
                Some(W::Install) => browser.workshop_install_selected(),
                Some(W::Uninstall) => browser.begin_workshop_uninstall(),
                Some(W::UpdateAll) => browser.workshop_update_all(),
                Some(W::Reload) => browser.reload_workshop_status(),
                None => match common(key) {
                    Some(O::Close) => browser.close_workshop(),
                    Some(O::Down) => browser.workshop_select_next(),
                    Some(O::Up) => browser.workshop_select_previous(),
                    _ => {}
                },
            }
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.confirm_install {
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
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.show_install_picker {
        // Install-location picker: Up/Down choose a library,
        // Enter installs the selected game into it, Esc cancels.
        use crate::keymap::InstallPickerAction as Ip;
        match own::<Ip>(key, None) {
            _ if input == KeyCode::Esc => browser.close_install_picker(),
            None => match common(key) {
                Some(O::Close) => browser.close_install_picker(),
                Some(O::Down) => browser.install_picker_next(),
                Some(O::Up) => browser.install_picker_previous(),
                _ => {}
            },
            Some(Ip::Install) => {
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
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.confirm_uninstall {
        // Uninstall confirmation prompt: y confirms, anything else cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // In the background: deleting a big game takes a while.
                // The finished job refreshes the game's install state,
                // or puts the error in the status bar.
                if let Some(id) = browser.selected().map(|game| game.id) {
                    browser.start_uninstall(id);
                }
                browser.confirm_uninstall = false;
            }
            _ => browser.confirm_uninstall = false,
        }
        return Ok(true);
    }
    if browser.show_dlc {
        // DLC overlay: navigate and toggle the highlighted DLC.
        use crate::keymap::DlcAction as Dl;
        match own::<Dl>(key, None) {
            _ if input == KeyCode::Esc => browser.close_dlc(),
            Some(action) => {
                if let Some(entry) = browser.selected_dlc() {
                    let _ = steam::set_dlc(entry.app_id as i32, action == Dl::Enable);
                    let _ = browser.refresh_dlc();
                }
            }
            None => match common(key) {
                Some(O::Close) => browser.close_dlc(),
                Some(O::Down) => browser.dlc_next(),
                Some(O::Up) => browser.dlc_previous(),
                _ => {}
            },
        }
        return Ok(true);
    }
    #[cfg(feature = "versioning")]
    if browser.show_branches {
        // Branches overlay: navigate and switch to the highlighted branch.
        use crate::keymap::BranchesAction as Br;
        match own::<Br>(key, None) {
            _ if input == KeyCode::Esc => browser.close_branches(),
            Some(Br::Switch) => {
                if let Some(branch) = browser.selected_branch() {
                    let _ = steam::set_branch(browser.branch_app_id(), &branch.name);
                    browser.close_branches();
                }
            }
            None => match common(key) {
                Some(O::Close) => browser.close_branches(),
                Some(O::Down) => browser.branch_next(),
                Some(O::Up) => browser.branch_previous(),
                _ => {}
            },
        }
        return Ok(true);
    }
    if browser.show_running {
        // Running overlay: navigate and stop the highlighted game.
        use crate::keymap::RunningAction as Ru;
        match own::<Ru>(key, None) {
            _ if input == KeyCode::Esc => browser.close_running(),
            Some(Ru::Stop) => {
                if let Some(r) = browser.selected_running() {
                    let _ = steam::stop(r.app_id as i32);
                    browser.refresh_running();
                }
            }
            None => match common(key) {
                Some(O::Close) => browser.close_running(),
                Some(O::Down) => browser.running_next(),
                Some(O::Up) => browser.running_previous(),
                _ => {}
            },
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.show_proton && browser.confirm_proton_uninstall {
        // Proton uninstall confirmation: y confirms (remove the
        // custom runtime + refresh), anything else cancels.
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = browser.do_proton_uninstall();
                browser.confirm_proton_uninstall = false;
            }
            _ => browser.confirm_proton_uninstall = false,
        }
        return Ok(true);
    }
    if browser.show_proton {
        // Proton overlay: navigate, set the highlighted runtime as
        // the global default, install it, or uninstall a custom one.
        use crate::keymap::ProtonAction as Pr;
        match own::<Pr>(key, None) {
            _ if input == KeyCode::Esc => browser.close_proton(),
            None => match common(key) {
                Some(O::Close) => browser.close_proton(),
                Some(O::Down) => browser.proton_next(),
                Some(O::Up) => browser.proton_previous(),
                _ => {}
            },
            Some(Pr::MakeDefault) => {
                if let Some(p) = browser.selected_proton() {
                    let _ = steam::proton_default(&p.name);
                    // Re-fetch so the [default] marker updates,
                    // keeping the current row highlighted.
                    let keep = browser.proton_index;
                    if browser.open_proton().is_ok() {
                        browser.proton_index = keep.min(browser.protons.len().saturating_sub(1));
                    }
                }
            }
            #[cfg(feature = "install")]
            Some(Pr::Install) => {
                // Queue a (long-running) download off the UI thread;
                // progress streams into the proton status cell.
                if let Some(p) = browser.selected_proton()
                    && !p.installed
                {
                    browser.clear_proton_status();
                    let _ = client.proton_install(&p.name, browser.proton_status.clone());
                }
            }
            #[cfg(feature = "install")]
            Some(Pr::Uninstall) => {
                // Only installed official runtimes are removable.
                if browser.selected_proton_uninstallable() {
                    browser.confirm_proton_uninstall = true;
                }
            }
            #[cfg(not(feature = "install"))]
            Some(Pr::Install | Pr::Uninstall) => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.installs.is_some() {
        if browser.installs_confirm_open() {
            match input {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    browser.answer_installs_confirm(true)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    browser.answer_installs_confirm(false)
                }
                _ => {}
            }
            return Ok(true);
        }
        if browser.transfer_editing() {
            match input {
                KeyCode::Enter | KeyCode::Esc => browser.transfer_path_done(),
                KeyCode::Backspace => browser.transfer_path_pop(),
                KeyCode::Char(c) => browser.transfer_path_push(c),
                _ => {}
            }
            return Ok(true);
        }
        if browser.transfer_confirm_open() {
            match input {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    browser.answer_transfer_confirm(client, true)?
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    browser.answer_transfer_confirm(client, false)?
                }
                _ => match common(key) {
                    Some(O::Down) => browser.installs_move(1),
                    Some(O::Up) => browser.installs_move(-1),
                    Some(O::PageDown) => browser.installs_move(10),
                    Some(O::PageUp) => browser.installs_move(-10),
                    _ => {}
                },
            }
            return Ok(true);
        }
        use crate::app::browser::installs::InstallsTab;
        use crate::keymap::InstallsAction as I;
        let group = match browser.installs.as_ref().map(|v| v.tab) {
            Some(InstallsTab::Duplicates) => "duplicates",
            Some(InstallsTab::Transfer) => "transfer",
            _ => "queue",
        };
        match own::<I>(key, Some(group)) {
            _ if input == KeyCode::Esc => browser.close_installs(),
            Some(I::Resume) => browser.installs_resume(client, false)?,
            Some(I::ResumeAll) => browser.installs_resume(client, true)?,
            Some(I::Pause) => browser.installs_signal(true),
            Some(I::Stop) => browser.installs_signal(false),
            Some(I::Forget) => browser.begin_installs_forget(false),
            Some(I::ForgetAndDelete) => browser.begin_installs_forget(true),
            Some(I::Rescan) => browser.rescan_duplicates(),
            Some(I::EditPath) => browser.transfer_edit_path(),
            Some(I::ExportAll) => browser.transfer_toggle_all(),
            Some(I::Export) => browser.transfer_export(),
            Some(I::Import) => browser.transfer_import(),
            None => match common(key) {
                Some(O::Close) => browser.close_installs(),
                Some(O::NextTab) => browser.installs_tab(true),
                Some(O::PreviousTab) => browser.installs_tab(false),
                Some(O::Down) => browser.installs_move(1),
                Some(O::Up) => browser.installs_move(-1),
                Some(O::PageDown) => browser.installs_move(10),
                Some(O::PageUp) => browser.installs_move(-10),
                None => {}
            },
        }
        return Ok(true);
    }
    // The per-game Store overlay can sit on top of the Steam Store one.
    if browser.game_info.is_some() {
        game_info_key(browser, key);
        return Ok(true);
    }
    #[cfg(feature = "store")]
    if browser.store.is_some() {
        store_key(browser, key);
        return Ok(true);
    }
    #[cfg(feature = "store")]
    if browser.shop.is_some() {
        shop_key(browser, key);
        return Ok(true);
    }
    if browser.show_depots {
        #[cfg(feature = "depot")]
        if browser.depots.files.is_some() {
            depot_files_key(browser, key);
            return Ok(true);
        }
        // Depots overlay: Esc/q close, j/k move, Enter opens the files,
        // Z opens the depot's manifest history on SteamDB.
        use crate::keymap::DepotsAction as D;
        match own::<D>(key, None) {
            _ if input == KeyCode::Esc => browser.close_depots(),
            Some(D::SteamDb) => {
                if let Some(depot) = browser.depots.selected_depot() {
                    let id = depot.id as i32;
                    browser.open_link(crate::app::browser::Link::SteamDbDepot, id);
                }
            }
            #[cfg(feature = "depot")]
            Some(D::Files) => browser.open_depot_files(),
            #[cfg(not(feature = "depot"))]
            Some(D::Files) => {}
            None => match common(key) {
                Some(O::Close) => browser.close_depots(),
                Some(O::Down) => browser.depots_next(),
                Some(O::Up) => browser.depots_previous(),
                _ => {}
            },
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.show_move {
        // Move prompt: type the destination library path, Enter
        // to relocate (kept open to show status), Esc to cancel.
        match input {
            KeyCode::Esc => browser.close_move(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                browser.do_move();
            }
            KeyCode::Backspace => browser.move_pop(),
            KeyCode::Char(c) => browser.move_push(c),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.show_relink {
        // Relink prompt: type the destination library path, Enter
        // to relink (kept open to show status), Esc to cancel.
        match input {
            KeyCode::Esc => browser.close_relink(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                browser.do_relink();
            }
            KeyCode::Backspace => browser.relink_pop(),
            KeyCode::Char(c) => browser.relink_push(c),
            _ => {}
        }
        return Ok(true);
    }
    #[cfg(feature = "install")]
    if browser.show_import {
        // Import prompt: type the library path holding the existing
        // files, Enter to register (kept open to show status), Esc
        // to cancel.
        match input {
            KeyCode::Esc => browser.close_import(),
            KeyCode::Char('\n') | KeyCode::Enter => {
                browser.do_import();
            }
            KeyCode::Backspace => browser.import_pop(),
            KeyCode::Char(c) => browser.import_push(c),
            _ => {}
        }
        return Ok(true);
    }
    if browser.show_help {
        // Help overlay: Esc/q/? close, j/k scroll.
        match common(key) {
            _ if matches!(input, KeyCode::Esc | KeyCode::Char('?')) => {
                browser.show_help = false;
            }
            Some(O::Close) => browser.show_help = false,
            Some(O::Down) => browser.help_scroll_down(),
            Some(O::Up) => browser.help_scroll_up(),
            _ => {}
        }
        return Ok(true);
    }
    if browser.show_account {
        // `o` logs out of Steam; any other key dismisses the overlay.
        match own::<crate::keymap::AccountAction>(key, None) {
            Some(crate::keymap::AccountAction::LogOut) => {
                let _ = steam::logout();
                browser.show_account = false;
                app.mode = Mode::Login;
            }
            _ => browser.show_account = false,
        }
        return Ok(true);
    }
    if browser.show_config {
        // Settings overlay: a confirm takes y/n; the prompt takes
        // text; otherwise j/k move, Enter edits, x unsets.
        if browser.settings.confirm.is_some() {
            browser.answer_settings_confirm(matches!(input, KeyCode::Char('y' | 'Y')));
        } else if browser.daemon_confirm_open() {
            browser.answer_daemon_stop(matches!(input, KeyCode::Char('y' | 'Y')));
        } else if browser.clean_confirm_open() {
            browser.answer_clean(matches!(input, KeyCode::Char('y' | 'Y')));
        } else if browser.settings.input.is_some() {
            match input {
                KeyCode::Esc => browser.cancel_settings_input(),
                KeyCode::Char('\n') | KeyCode::Enter => browser.commit_settings_input(),
                KeyCode::Backspace => browser.settings_input_pop(),
                KeyCode::Char(c) => browser.settings_input_push(c),
                _ => {}
            }
        } else {
            use crate::app::browser::settings::Tab;
            use crate::keymap::SettingsAction as S;
            let tab = browser.settings.tab;
            let group = match tab {
                Tab::Settings => "settings",
                Tab::Profiles => "profiles",
                Tab::Check => "check",
                Tab::Daemons => "daemons",
                Tab::Clean => "clean",
            };
            match own::<S>(key, Some(group)) {
                _ if input == KeyCode::Esc => browser.close_config(),
                Some(S::Edit) => browser.settings_activate(),
                Some(S::Unset) => browser.settings_unset(),
                Some(S::MakeDefault) => browser.make_profile_default(),
                Some(S::CheckAgain) => browser.rerun_check(),
                Some(S::StopDaemon) => browser.begin_daemon_stop(false),
                Some(S::StopAllDaemons) => browser.begin_daemon_stop(true),
                Some(S::RefreshDaemons) => browser.reload_daemons(),
                Some(S::Clean) => browser.begin_clean(),
                Some(S::AllShaders) => browser.toggle_clean_all_shaders(),
                Some(S::RescanClean) => browser.rescan_clean(),
                None => match (tab, common(key)) {
                    (_, Some(O::Close)) => browser.close_config(),
                    (_, Some(O::NextTab)) => browser.settings_tab(true),
                    (_, Some(O::PreviousTab)) => browser.settings_tab(false),
                    (Tab::Settings, Some(O::Down)) => browser.settings_move(1),
                    (Tab::Settings, Some(O::Up)) => browser.settings_move(-1),
                    (Tab::Profiles, Some(O::Down)) => browser.profiles_move(1),
                    (Tab::Profiles, Some(O::Up)) => browser.profiles_move(-1),
                    (Tab::Profiles, Some(O::PageDown)) => browser.settings_scroll(10),
                    (Tab::Profiles, Some(O::PageUp)) => browser.settings_scroll(-10),
                    (Tab::Check, Some(O::Down)) => browser.settings_scroll(1),
                    (Tab::Check, Some(O::Up)) => browser.settings_scroll(-1),
                    (Tab::Check, Some(O::PageDown)) => browser.settings_scroll(10),
                    (Tab::Check, Some(O::PageUp)) => browser.settings_scroll(-10),
                    (Tab::Daemons, Some(O::Down)) => browser.daemons_move(1),
                    (Tab::Daemons, Some(O::Up)) => browser.daemons_move(-1),
                    _ => {}
                },
            }
        }
        return Ok(true);
    }
    if browser.show_wallet {
        // Any key dismisses the wallet overlay.
        browser.show_wallet = false;
        return Ok(true);
    }
    if browser.filtering {
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
        return Ok(true);
    }
    Ok(false)
}

/// Keys in the depot file view. With a prompt open they type into it;
/// otherwise Esc/q go back to the depot list.
#[cfg(feature = "depot")]
fn depot_files_key(browser: &mut crate::app::browser::Browser, key: Key) {
    let input = key.code;
    if browser.depot_input_open() {
        match input {
            KeyCode::Esc => browser.cancel_depot_input(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.submit_depot_input(),
            KeyCode::Backspace => browser.depot_input_pop(),
            KeyCode::Char(c) => browser.depot_input_push(c),
            _ => {}
        }
        return;
    }
    use crate::keymap::DepotFilesAction as F;
    match own::<F>(key, None) {
        _ if input == KeyCode::Esc => browser.close_depot_files(),
        Some(F::Filter) => browser.begin_depot_filter(),
        Some(F::Branch) => browser.begin_depot_branch(),
        Some(F::Download) => browser.begin_depot_download(),
        None => match common(key) {
            Some(O::Close) => browser.close_depot_files(),
            Some(O::Down) => browser.depot_files_move(1),
            Some(O::Up) => browser.depot_files_move(-1),
            Some(O::PageDown) => browser.depot_files_move(10),
            Some(O::PageUp) => browser.depot_files_move(-10),
            _ => {}
        },
    }
}

/// Keys in the Store overlay. With the regions prompt open they type into
/// it.
#[cfg(feature = "store")]
fn store_key(browser: &mut crate::app::browser::Browser, key: Key) {
    let input = key.code;
    if browser.store_input_open() {
        match input {
            KeyCode::Esc => browser.cancel_store_input(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.submit_store_input(),
            KeyCode::Backspace => browser.store_input_pop(),
            KeyCode::Char(c) => browser.store_input_push(c),
            _ => {}
        }
        return;
    }
    use crate::keymap::StoreAction as St;
    match own::<St>(key, None) {
        _ if input == KeyCode::Esc => browser.close_store(),
        _ if key == Key::plain(KeyCode::Right) => browser.store_cycle(1),
        _ if key == Key::plain(KeyCode::Left) => browser.store_cycle(-1),
        Some(St::NextSection) => browser.store_cycle(1),
        Some(St::PreviousSection) => browser.store_cycle(-1),
        Some(St::Regions) => browser.begin_store_regions(),
        None => match common(key) {
            Some(O::Close) => browser.close_store(),
            Some(O::NextTab) => browser.store_cycle(1),
            Some(O::PreviousTab) => browser.store_cycle(-1),
            Some(O::Down) => browser.store_scroll(1),
            Some(O::Up) => browser.store_scroll(-1),
            Some(O::PageDown) => browser.store_scroll(10),
            Some(O::PageUp) => browser.store_scroll(-10),
            None => {}
        },
    }
}

/// The Steam Store overlay or the per-game Store overlay is open.
#[cfg(feature = "social")]
fn store_overlay_open(browser: &crate::app::browser::Browser) -> bool {
    #[cfg(feature = "store")]
    {
        browser.shop.is_some() || browser.store.is_some()
    }
    #[cfg(not(feature = "store"))]
    {
        let _ = browser;
        false
    }
}

/// Keys in the Steam Store overlay: the search prompt and the wishlist
/// confirm take them first.
#[cfg(feature = "store")]
fn shop_key(browser: &mut crate::app::browser::Browser, key: Key) {
    let input = key.code;
    if browser.shop_confirm_open() {
        match input {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                browser.answer_shop_confirm(true)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                browser.answer_shop_confirm(false)
            }
            _ => {}
        }
        return;
    }
    if browser.shop_input_open() {
        match input {
            KeyCode::Esc => browser.cancel_shop_input(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.submit_shop_input(),
            KeyCode::Backspace => browser.shop_input_pop(),
            KeyCode::Tab => {
                browser.cancel_shop_input();
                browser.shop_cycle(1);
            }
            KeyCode::BackTab => {
                browser.cancel_shop_input();
                browser.shop_cycle(-1);
            }
            KeyCode::Char(c) => browser.shop_input_push(c),
            _ => {}
        }
        return;
    }
    use crate::keymap::SteamStoreAction as Sh;
    match own::<Sh>(key, None) {
        _ if input == KeyCode::Esc => browser.close_shop(),
        _ if key == Key::plain(KeyCode::Right) => browser.shop_cycle(1),
        _ if key == Key::plain(KeyCode::Left) => browser.shop_cycle(-1),
        Some(Sh::NextSection) => browser.shop_cycle(1),
        Some(Sh::PreviousSection) => browser.shop_cycle(-1),
        Some(Sh::Details) => browser.shop_open_game(),
        Some(Sh::Search) => browser.begin_shop_search(),
        Some(Sh::Wishlist) => browser.begin_shop_wish(),
        Some(Sh::StorePage) => browser.shop_open_page(),
        Some(Sh::NextDeals) => browser.shop_next_deals(),
        Some(Sh::Reload) => browser.shop_reload(),
        None => match common(key) {
            Some(O::Close) => browser.close_shop(),
            Some(O::NextTab) => browser.shop_cycle(1),
            Some(O::PreviousTab) => browser.shop_cycle(-1),
            Some(O::Down) => browser.shop_move(1),
            Some(O::Up) => browser.shop_move(-1),
            Some(O::PageDown) => browser.shop_move(10),
            Some(O::PageUp) => browser.shop_move(-10),
            None => {}
        },
    }
}

/// The Workshop comments sub-pane: scroll, or write a comment (`c`), which
/// asks before it posts.
#[cfg(feature = "workshop")]
fn workshop_comments_key(browser: &mut crate::app::browser::Browser, key: Key) {
    let input = key.code;
    match browser
        .workshop_tasks
        .comment
        .as_ref()
        .map(|d| d.confirming)
    {
        Some(true) => browser.answer_workshop_comment(matches!(input, KeyCode::Char('y' | 'Y'))),
        Some(false) => match input {
            KeyCode::Esc => browser.cancel_workshop_comment(),
            KeyCode::Enter | KeyCode::Char('\n') => browser.submit_workshop_comment(),
            KeyCode::Backspace => browser.workshop_comment_pop(),
            KeyCode::Char(c) => browser.workshop_comment_push(c),
            _ => {}
        },
        // No text is edited here, so j/k are also scroll keys.
        None => {
            use crate::keymap::WorkshopCommentsAction as Wc;
            match own::<Wc>(key, None) {
                _ if input == KeyCode::Esc => browser.close_workshop_comments(),
                Some(Wc::Write) => browser.begin_workshop_comment(),
                None => match common(key) {
                    Some(O::Close) => browser.close_workshop_comments(),
                    Some(O::Down) => browser.workshop_comments_scroll_down(),
                    Some(O::Up) => browser.workshop_comments_scroll_up(),
                    _ => {}
                },
            }
        }
    }
}

/// The Game info overlay: `Tab` switches tabs, `j`/`k` and `PgUp`/`PgDn`
/// scroll, `r` re-runs the checks, `s` saves the artwork and `o` edits its
/// folder (a prompt).
fn game_info_key(browser: &mut crate::app::browser::Browser, key: Key) {
    use crate::app::browser::game_info::InfoTab;
    use crate::keymap::GameInfoAction as G;
    let input = key.code;
    let Some(view) = &browser.game_info else {
        return;
    };
    if view.dir_input.is_some() {
        match input {
            KeyCode::Esc => browser.cancel_artwork_dir(),
            KeyCode::Char('\n') | KeyCode::Enter => browser.commit_artwork_dir(),
            KeyCode::Backspace => browser.artwork_dir_pop(),
            KeyCode::Char(c) => browser.artwork_dir_push(c),
            _ => {}
        }
        return;
    }
    let group = match view.tab {
        InfoTab::Details => "details",
        InfoTab::Checks => "checks",
        InfoTab::Artwork => "artwork",
    };
    match own::<G>(key, Some(group)) {
        _ if input == KeyCode::Esc => browser.close_game_info(),
        Some(G::CheckAgain) => browser.rerun_game_checks(),
        Some(G::SaveArtwork) => browser.save_game_artwork(),
        Some(G::ArtworkFolder) => browser.begin_artwork_dir(),
        None => match common(key) {
            Some(O::Close) => browser.close_game_info(),
            Some(O::NextTab) => browser.game_info_tab(true),
            Some(O::PreviousTab) => browser.game_info_tab(false),
            Some(O::Down) => browser.game_info_scroll(1),
            Some(O::Up) => browser.game_info_scroll(-1),
            Some(O::PageDown) => browser.game_info_scroll(10),
            Some(O::PageUp) => browser.game_info_scroll(-10),
            None => {}
        },
    }
}

/// The `[overlay]` action `key` runs; the arrow keys always move.
fn common(key: Key) -> Option<O> {
    match key {
        k if k == Key::plain(KeyCode::Down) => Some(O::Down),
        k if k == Key::plain(KeyCode::Up) => Some(O::Up),
        k => keymap::get().overlay_action(k),
    }
}

/// The action `key` runs in overlay context `A`, in tab `group`.
fn own<A: Actions>(key: Key, group: Option<&str>) -> Option<A> {
    keymap::get().find(key, group)
}
