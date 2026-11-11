//! A custom `keybinds.toml` changes what the help, the status-bar hints and
//! the Actions menu show. The keymap is process-global, so this file holds
//! one test that installs it before anything reads it.

use gaki_tui::app::browser::{Action, Browser};
use gaki_tui::app::game::Game;
use gaki_tui::app::steam::LibraryGame;
use gaki_tui::keymap::{self, Keymap, LibraryAction};
use gaki_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render<F: FnOnce(&mut ratatui::Frame)>(w: u16, h: u16, draw: F) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(draw).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn a_custom_keymap_shows_everywhere() {
    let dir = std::env::temp_dir().join("gaki-tui-keymap-live");
    libgaki::core::config::set_config_dir(dir.clone());
    // SAFETY: set before anything reads it, in this single-test process.
    unsafe {
        std::env::set_var("XDG_CACHE_HOME", &dir);
    }
    keymap::install(
        Keymap::parse(
            "[library]\nfilter = \"ctrl+f\"\nfavourite = \"*\"\nhide = []\nsteam_page = [\"e\", \"F5\"]\nachievements = \"alt+a\"\n[installs]\npause = \"z\"\n[overlay]\ndown = [\"j\", \"ctrl+n\"]\n[friends]\nprofile = \"alt+v\"\n[dlc]\nenable = \"+\"\n",
        )
        .unwrap(),
    );
    assert_eq!(keymap::get().label(LibraryAction::SteamPage), "e / F5");

    let help = render(100, 120, |f| {
        f.render_widget(ui::help::help(0, f.area().width), f.area())
    });
    assert!(help.contains("ctrl+f"), "{help}");
    assert!(help.contains("e / F5"), "{help}");
    assert!(help.contains("alt+a"), "{help}");
    #[cfg(feature = "social")]
    assert!(help.contains("alt+v"), "{help}");

    let dlc = render(80, 10, |f| {
        let browser = Browser::new(Vec::new());
        f.render_widget(ui::dlc::dlc(&browser), f.area())
    });
    assert!(dlc.contains("[+] enable"), "{dlc}");

    let browser = Browser::new(vec![Game::from_library(LibraryGame {
        app_id: 1,
        name: "Alpha".into(),
        is_installed: true,
        install_path: None,
        update_available: false,
        is_owned: true,
        is_family_shared: false,
        platform: None,
        active_branch: None,
        assets: None,
        store_url: None,
        playtime_minutes: None,
        playtime_2weeks_minutes: None,
        last_played: None,
        size_on_disk: None,
    })]);
    let footer = render(160, 2, |f| {
        f.render_widget(ui::status::status_bar(&browser, None), f.area())
    });
    assert!(footer.contains("[ctrl+f] filter"), "{footer}");
    assert!(footer.contains("[*] fav"), "{footer}");

    #[cfg(feature = "install")]
    {
        let view = gaki_tui::app::browser::installs::InstallsView::with_rows(Vec::new());
        let installs = render(160, 10, |f| ui::installs::draw(f, f.area(), &view));
        assert!(installs.contains("[z] pause"), "{installs}");
        assert!(!installs.contains("[p] pause"), "{installs}");
    }
    assert_eq!(
        keymap::get().overlay_action(gaki_tui::keymap::Key::ctrl('n')),
        Some(gaki_tui::keymap::OverlayAction::Down)
    );

    let rows = browser.action_candidates();
    let key = |action| rows.iter().find(|r| r.action == action).unwrap().key();
    assert_eq!(key(Action::SteamPage), "e");
    assert_eq!(key(Action::Hide), "");
    assert_eq!(key(Action::Achievements), "alt+a");
}
