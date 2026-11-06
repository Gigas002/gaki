//! Exercises the per-game Actions menu model (applicability + filtering) and
//! renders the new 0.1.16/0.1.17 overlays (actions, versions, game settings,
//! collections, runtime plugins) into tui's off-screen `TestBackend` to prove
//! they lay out and paint without panicking. State is set through the public
//! fields directly so the tests never reach Steam.

use gaki_tui::app::browser::{Action, Browser, Link};
use gaki_tui::app::game::Game;
#[cfg(feature = "collections")]
use gaki_tui::app::steam::Collection;
#[cfg(feature = "versioning")]
use gaki_tui::app::steam::{DepotManifestInfo, PinState};
use gaki_tui::app::steam::{
    GameSettings, LaunchTarget, LauncherSettings, LibraryGame, ProxySettings,
};
use gaki_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
#[cfg(feature = "versioning")]
use std::collections::BTreeMap;

fn isolate_config() {
    let dir = std::env::temp_dir().join("gaki-tui-test-config");
    // A repeat call in the same process keeps the first dir, which is fine.
    libgaki::core::config::set_config_dir(dir.clone());
    // SAFETY: set before any Config::cached() call in this test process.
    unsafe {
        std::env::set_var("XDG_CACHE_HOME", &dir);
    }
}

fn game(id: u32, name: &str, installed: bool) -> Game {
    Game::from_library(LibraryGame {
        app_id: id,
        name: name.to_string(),
        is_installed: installed,
        install_path: None,
        update_available: false,
        is_owned: true,
        is_family_shared: false,
        platform: None,
        active_branch: None,
        assets: None,
        store_url: None,
    })
}

/// Flatten a rendered `TestBackend` buffer into one string for assertions.
fn render<F: FnOnce(&mut ratatui::Frame)>(w: u16, h: u16, draw: F) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[cfg(all(feature = "versioning", feature = "collections"))]
#[test]
fn action_candidates_depend_on_install_state() {
    isolate_config();

    // An installed game offers Play/Update/Verify/Uninstall, not Install.
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    let actions: Vec<Action> = browser
        .action_candidates()
        .iter()
        .map(|r| r.action)
        .collect();
    assert!(actions.contains(&Action::Play), "installed game can Play");
    assert!(
        actions.contains(&Action::Update),
        "installed game can Update"
    );
    assert!(
        actions.contains(&Action::Uninstall),
        "installed game can Uninstall"
    );
    assert!(actions.contains(&Action::Move), "installed game can Move");
    assert!(
        !actions.contains(&Action::Install),
        "installed game hides Install"
    );

    // Every game offers the new feature actions regardless of install state.
    assert!(
        actions.contains(&Action::Versions),
        "Versions & pinning listed"
    );
    assert!(actions.contains(&Action::Collections), "Collections listed");
    assert!(
        actions.contains(&Action::GameSettings),
        "Game settings listed"
    );

    // A not-installed game offers Install, not Play/Uninstall/Move.
    browser = Browser::new(vec![game(2, "Beta", false)]);
    let actions: Vec<Action> = browser
        .action_candidates()
        .iter()
        .map(|r| r.action)
        .collect();
    assert!(
        actions.contains(&Action::Install),
        "uninstalled game can Install"
    );
    assert!(
        !actions.contains(&Action::Play),
        "uninstalled game hides Play"
    );
    assert!(
        !actions.contains(&Action::Uninstall),
        "uninstalled game hides Uninstall"
    );
    assert!(
        !actions.contains(&Action::Move),
        "uninstalled game hides Move"
    );
}

#[test]
fn actions_filter_narrows_and_selection_tracks() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.open_actions();
    let full = browser.filtered_actions().len();
    assert!(full > 5, "menu lists many actions when unfiltered");

    // Typing narrows to the matching rows and resets the highlight.
    // (Favourite is in every build.)
    for c in "favour".chars() {
        browser.actions_filter_push(c);
    }
    let filtered = browser.filtered_actions();
    assert_eq!(filtered.len(), 1, "only Favourite matches 'favour'");
    assert_eq!(filtered[0].action, Action::Favourite);
    assert_eq!(browser.selected_action(), Some(Action::Favourite));

    browser.actions_filter_pop();
    assert!(
        !browser.filtered_actions().is_empty(),
        "backspacing the filter re-widens the list"
    );
}

#[cfg(feature = "versioning")]
#[test]
fn actions_menu_renders_categories() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.open_actions();
    let text = render(70, 30, |f| {
        f.render_widget(ui::actions::actions(&browser), Rect::new(0, 0, 70, 30));
    });
    assert!(text.contains("Actions"), "menu title renders");
    assert!(text.contains("Alpha"), "menu title names the game");
    assert!(text.contains("Versions"), "a category header renders");
    assert!(
        text.contains("pinning"),
        "the versions/pinning action label renders"
    );
}

#[cfg(feature = "versioning")]
#[test]
fn versions_overlay_renders_pin_and_manifest() {
    isolate_config();
    let mut browser = Browser::new(vec![game(730, "CS", true)]);
    browser.show_versions = true;
    browser.versions_manifests = vec![DepotManifestInfo {
        depot_id: 731,
        depot_name: Some("CS content".to_string()),
        branch: "public".to_string(),
        manifest_id: 12345,
        size: 1024,
    }];
    let mut pinned = BTreeMap::new();
    pinned.insert("731".to_string(), 12345u64);
    browser.versions_available = Some(PinState {
        app_id: 730,
        available: true,
        install_path: Some("C:/games/cs".to_string()),
        pinned: true,
        pinned_manifests: pinned,
    });
    let text = render(80, 20, |f| {
        f.render_widget(ui::versions::versions(&browser), Rect::new(0, 0, 80, 20));
    });
    assert!(text.contains("PINNED"), "pinned banner renders");
    assert!(text.contains("12345"), "the depot manifest id renders");
    assert!(text.contains("731"), "the depot id renders");
}

#[test]
fn game_settings_overlay_renders_rows() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.show_game_config = true;
    browser.game_config = Some(GameSettings {
        app_id: 1,
        profile: None,
        platform: Some("windows".to_string()),
        steam: None,
        effective_profile: Some("proton".to_string()),
        effective_steam: "host".to_string(),
        wrappers: vec!["gamemoderun".to_string(), "mangohud".to_string()],
        game_wrappers: vec!["mangohud".to_string()],
        profiles: vec!["proton".to_string()],
    });
    let text = render(70, 12, |f| {
        f.render_widget(
            ui::game_config::game_config(&browser),
            Rect::new(0, 0, 70, 12),
        );
    });
    assert!(
        text.contains("Profile: (default: proton)"),
        "an unset profile shows the default"
    );
    assert!(text.contains("Platform: windows"), "platform renders");
    assert!(
        text.contains("Steam: (profile: host)"),
        "an unset Steam integration shows the profile's"
    );
    assert!(
        text.contains("Wrappers: gamemoderun mangohud"),
        "wrappers render"
    );
    for gone in ["Forced Proton", "Runner", "Launch script"] {
        assert!(!text.contains(gone), "{gone} row is gone");
    }
}

#[test]
fn launch_targets_parse_and_map_to_play_opts() {
    let target = LaunchTarget {
        id: "windows-host".into(),
        platform: "windows".into(),
        steam_mode: Some("host".into()),
        available: true,
        flags: ["--platform", "windows", "--steam"]
            .map(String::from)
            .to_vec(),
        ..Default::default()
    };
    assert_eq!(target.label(), "Windows · host Steam");
    let opts = target.play_opts();
    assert_eq!(opts.platform.as_deref(), Some("windows"));
    assert!(opts.steam, "the host target bridges to Steam");

    let auto = LaunchTarget {
        platform: "windows".into(),
        steam_mode: Some("auto".into()),
        flags: ["--platform", "windows"].map(String::from).to_vec(),
        ..Default::default()
    };
    assert_eq!(auto.label(), "Windows · Steam if running");
    assert!(!auto.play_opts().steam);

    let native = LaunchTarget {
        platform: "linux".into(),
        flags: ["--platform", "linux", "--steam"]
            .map(String::from)
            .to_vec(),
        ..Default::default()
    };
    assert!(native.play_opts().steam, "native --steam flag carries over");
}

#[test]
fn launch_targets_overlay_renders_targets() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.show_launch_targets = true;
    browser.launch_targets = vec![
        LaunchTarget {
            id: "linux".to_string(),
            platform: "linux".to_string(),
            available: true,
            default: true,
            ..Default::default()
        },
        LaunchTarget {
            id: "windows".to_string(),
            platform: "windows".to_string(),
            steam_mode: Some("auto".to_string()),
            missing: Some("build".to_string()),
            ..Default::default()
        },
    ];
    let text = render(70, 10, |f| {
        f.render_widget(
            ui::launch_targets::launch_targets(&browser),
            Rect::new(0, 0, 70, 10),
        );
    });
    assert!(text.contains("Linux (native)"), "native target renders");
    assert!(text.contains("[default]"), "default target is tagged");
    assert!(
        text.contains("missing: build"),
        "unavailable target says why"
    );

    // The unavailable target refuses to launch and says so.
    browser.launch_targets_index = 1;
    assert!(browser.selected_launch_target().is_none());
    assert!(browser.launch_targets_status.contains("build"));
    browser.launch_targets_index = 0;
    assert!(browser.selected_launch_target().is_some());
}

#[cfg(feature = "collections")]
#[test]
fn collections_overlay_renders_list() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.show_collections = true;
    browser.collections = vec![Collection {
        id: "uc-1".to_string(),
        name: "Favourites".to_string(),
        kind: "static".to_string(),
        dynamic: false,
        count: Some(7),
        app_ids: vec![1, 2, 3],
    }];
    let text = render(70, 12, |f| {
        f.render_widget(
            ui::collections::collections(&browser),
            Rect::new(0, 0, 70, 12),
        );
    });
    assert!(text.contains("Collections"), "overlay title renders");
    assert!(text.contains("Favourites"), "collection name renders");
    assert!(text.contains("7 games"), "collection count renders");
}

#[test]
fn settings_overlay_renders_proxy() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.show_config = true;
    browser.config_info = Some(LauncherSettings::default());
    browser.config_proxy = Some(ProxySettings {
        url: Some("http://host:8080".to_string()),
        no_proxy: Some("localhost".to_string()),
    });
    let text = render(80, 18, |f| {
        f.render_widget(ui::config::config(&browser), Rect::new(0, 0, 80, 18));
    });
    assert!(text.contains("Proxy"), "proxy row renders");
    assert!(text.contains("http://host:8080"), "proxy url renders");
    assert!(text.contains("localhost"), "proxy bypass renders");

    // Editing shows the typed URL plus the edit-mode title hint.
    browser.config_proxy_input = Some("http://new:1".to_string());
    let text = render(80, 18, |f| {
        f.render_widget(ui::config::config(&browser), Rect::new(0, 0, 80, 18));
    });
    assert!(
        text.contains("http://new:1"),
        "typed url shown while editing"
    );
    assert!(text.contains("save"), "edit-mode title hint shown");
}

#[test]
fn every_game_links_to_steam_protondb_and_the_proton_tracker() {
    isolate_config();
    for installed in [true, false] {
        let browser = Browser::new(vec![game(1, "Alpha", installed)]);
        let rows = browser.action_candidates();
        for (action, key) in [
            (Action::SteamPage, "E"),
            (Action::ProtonDb, "B"),
            (Action::ProtonIssues, "T"),
        ] {
            let row = rows.iter().find(|r| r.action == action).expect("link row");
            assert_eq!(row.category, "Links");
            assert_eq!(row.key, key);
        }
    }
    assert_eq!(
        Link::Steam.url(945360),
        "https://store.steampowered.com/app/945360"
    );
    assert_eq!(
        Link::ProtonDb.url(945360),
        "https://www.protondb.com/app/945360"
    );
    assert!(Link::ProtonIssues.url(945360).ends_with("+945360"));
}
