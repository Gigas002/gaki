//! The overlays that float over the library, drawn in a fixed order so a
//! confirmation prompt always sits above the overlay that asked for it.

use ratatui::Frame;
use ratatui::widgets::Clear;

use crate::app::browser::Browser;
use crate::ui;
use crate::utils::stateful::Named;

/// Draw every open overlay over the library.
pub fn draw(frame: &mut Frame, browser: &Browser) {
    // Achievements overlay floats above everything.
    if browser.show_achievements {
        let area = ui::centered_rect(72, 82, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::achievements::achievements(browser), area);
    }

    // Chat view floats above everything (the in-app conversation
    // overlay; a chat can also be popped into its own terminal).
    if browser.show_chat {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::chat::chat(browser, area.width, area.height), area);
    }

    // Inventory overlay floats above everything.
    if browser.show_inventory {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::inventory::inventory(browser), area);
    }

    // Market listings overlay floats above everything.
    if browser.show_market {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::market::market(browser), area);
    }

    // Community Market search overlay floats above everything.
    if browser.show_market_search {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        // Pass the inner content height (minus the block border) so the
        // result window never overflows the overlay.
        let rows = area.height.saturating_sub(2) as usize;
        frame.render_widget(ui::market_search::market_search(browser, rows), area);
    }

    // Workshop overlay floats above everything.
    if browser.show_workshop {
        let area = ui::centered_rect(72, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::workshop::workshop(browser), area);
    }

    // Help overlay floats above everything.
    if browser.show_help {
        let area = ui::centered_rect(64, 84, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::help::help(browser.help_scroll), area);
    }

    // Steam Cloud overlay floats above everything.
    if browser.show_cloud {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::cloud::cloud(browser), area);
    }

    // Launch-options overlay floats above everything.
    if browser.show_launch {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::launch::launch(browser), area);
    }

    // Account overlay floats above everything.
    if browser.show_account {
        let area = ui::centered_rect(50, 55, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::account::account(browser), area);
    }

    // Settings overlay floats above everything.
    if browser.show_config {
        let area = ui::centered_rect(55, 60, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::config::config(browser), area);
    }

    // Wallet overlay floats above everything.
    if browser.show_wallet {
        let area = ui::centered_rect(40, 30, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::wallet::wallet(browser), area);
    }

    // Per-game Actions menu (command palette) floats above the library.
    if browser.show_actions {
        let area = ui::centered_rect(60, 78, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::actions::actions(browser), area);
    }

    // Versions & pinning overlay floats above everything.
    if browser.show_versions {
        let area = ui::centered_rect(72, 78, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::versions::versions(browser), area);
    }

    // Per-game settings overlay floats above everything.
    if browser.show_game_config {
        let area = ui::centered_rect(58, 45, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::game_config::game_config(browser), area);
    }

    // Collections overlay floats above everything.
    if browser.show_collections {
        let area = ui::centered_rect(58, 62, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::collections::collections(browser), area);
    }

    // Launch targets ("Play as…") overlay floats above everything.
    if browser.show_launch_targets {
        let area = ui::centered_rect(58, 40, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::launch_targets::launch_targets(browser), area);
    }

    // "Not installed — install now?" prompt floats above the library.
    if browser.confirm_install
        && let Some(game) = browser.selected()
    {
        let area = ui::centered_rect(40, 20, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::confirm::confirm_install(&game.get_name()), area);
    }

    // Install-location picker floats above the library.
    if browser.show_install_picker {
        let area = ui::centered_rect(64, 40, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::install_picker::install_picker(browser), area);
    }

    // Uninstall confirmation prompt floats above the library.
    if browser.confirm_uninstall
        && let Some(game) = browser.selected()
    {
        let area = ui::centered_rect(40, 20, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::confirm::confirm_uninstall(&game.get_name()), area);
    }

    // DLC overlay floats above everything.
    if browser.show_dlc {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::dlc::dlc(browser), area);
    }

    // Branches overlay floats above everything.
    if browser.show_branches {
        let area = ui::centered_rect(60, 60, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::branches::branches(browser), area);
    }

    // Depots overlay floats above everything.
    if browser.show_depots {
        let area = ui::centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::depots::depots(browser), area);
    }

    // Move (relocate install) prompt floats above everything.
    if browser.show_move {
        let area = ui::centered_rect(60, 25, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::move_game::move_overlay(browser), area);
    }

    // Relink (relink install) prompt floats above everything.
    if browser.show_relink {
        let area = ui::centered_rect(60, 25, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::relink::relink_overlay(browser), area);
    }

    // Import (register existing install) prompt floats above everything.
    if browser.show_import {
        let area = ui::centered_rect(60, 25, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::import::import_overlay(browser), area);
    }

    // Proton runtimes overlay floats above everything.
    if browser.show_proton {
        let area = ui::centered_rect(60, 70, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::proton::proton(browser), area);

        // Destructive uninstall confirmation floats above the overlay.
        if browser.confirm_proton_uninstall
            && let Some(p) = browser.selected_proton()
        {
            let prompt = ui::confirm::confirm_uninstall(&p.name);
            let confirm_area = ui::centered_rect(40, 20, frame.area());
            frame.render_widget(Clear, confirm_area);
            frame.render_widget(prompt, confirm_area);
        }
    }

    // Running-games overlay floats above everything.
    if browser.show_running {
        let area = ui::centered_rect(60, 60, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::running::running(browser), area);
    }

    // Add-friend (search/add) prompt floats above everything.
    if browser.show_friend_add {
        let area = ui::centered_rect(60, 30, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(ui::friend_add::friend_add_overlay(browser), area);
    }

    // Remove-friend confirmation prompt floats above the friends panel.
    if browser.confirm_friend_remove
        && let Some(friend) = browser.selected_friend()
    {
        let area = ui::centered_rect(40, 20, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            ui::confirm::confirm_remove_friend(&friend.display_name()),
            area,
        );
    }
}
