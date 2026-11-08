//! Exercises the per-game Actions menu model (applicability + filtering) and
//! renders the new 0.1.16/0.1.17 overlays (actions, versions, game settings,
//! collections, runtime plugins) into tui's off-screen `TestBackend` to prove
//! they lay out and paint without panicking. State is set through the public
//! fields directly so the tests never reach Steam.

use gaki_tui::app::browser::settings::{CleanTab, Pending, SPECS, SettingsView, Tab};
use gaki_tui::app::browser::{Action, Browser, Link};
use gaki_tui::app::game::Game;
#[cfg(feature = "collections")]
use gaki_tui::app::steam::Collection;
#[cfg(feature = "versioning")]
use gaki_tui::app::steam::{DepotManifestInfo, PinState};
use gaki_tui::app::steam::{GameSettings, LaunchTarget, LibraryGame};
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

#[cfg(feature = "collections")]
fn favourites() -> Collection {
    Collection {
        id: "uc-1".to_string(),
        name: "Favourites".to_string(),
        kind: "static".to_string(),
        dynamic: false,
        count: Some(7),
        app_ids: vec![1, 2, 3],
    }
}

/// `e` opens a rename prompt prefilled with the name; an empty name is
/// refused and keeps the prompt; Esc drops it. (Committing a real rename
/// would write the collections store, so it stays out of this test.)
#[cfg(feature = "collections")]
#[test]
fn collection_rename_prompt() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.show_collections = true;
    browser.collections = vec![favourites()];

    browser.begin_collection_rename();
    assert_eq!(browser.collections_input.as_deref(), Some("Favourites"));
    assert_eq!(browser.collections_renaming.as_deref(), Some("Favourites"));
    let draw = |browser: &Browser| {
        render(80, 12, |f| {
            f.render_widget(
                ui::collections::collections(browser),
                Rect::new(0, 0, 80, 12),
            );
        })
    };
    let text = draw(&browser);
    assert!(
        text.contains("Rename \"Favourites\" to: Favourites_"),
        "{text}"
    );

    for _ in 0.."Favourites".len() {
        browser.collections_input_pop();
    }
    browser.commit_collection_input();
    assert!(
        browser.collections_input.is_some(),
        "empty name keeps the prompt"
    );
    assert!(draw(&browser).contains("can't be empty"));

    browser.cancel_collection_input();
    assert!(browser.collections_input.is_none());
    assert!(browser.collections_renaming.is_none());
}

/// `x` asks before deleting; "no" leaves the collection alone.
#[cfg(feature = "collections")]
#[test]
fn collection_delete_asks_first() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.show_collections = true;
    browser.collections = vec![favourites()];

    browser.begin_collection_delete();
    assert_eq!(browser.collections_delete.as_deref(), Some("Favourites"));
    let text = render(90, 12, |f| {
        f.render_widget(
            ui::collections::collections(&browser),
            Rect::new(0, 0, 90, 12),
        );
    });
    assert!(
        text.contains("Delete the collection \"Favourites\"?"),
        "{text}"
    );

    browser.answer_collection_delete(false);
    assert!(browser.collections_delete.is_none());
    assert_eq!(browser.collections.len(), 1, "nothing deleted");
    assert!(browser.collections_status.is_empty());
}

fn settings_screen(view: &SettingsView, width: u16, height: u16) -> String {
    render(width, height, |f| {
        ui::config::draw(f, Rect::new(0, 0, width, height), view)
    })
}

fn key_index(key: &str) -> usize {
    SPECS.iter().position(|s| s.key == key).unwrap()
}

/// Every key is listed; a set key shows its value, an unset one its
/// default; the footer describes the highlighted key.
#[test]
fn settings_list_every_key_with_values_and_defaults() {
    let mut values = vec![None; SPECS.len()];
    values[key_index("proxy")] = Some("http://host:8080".to_string());
    let mut view = SettingsView::with_values(values);
    view.selected = key_index("proxy");
    let text = settings_screen(&view, 110, 26);
    for spec in SPECS {
        assert!(text.contains(spec.label), "{} listed: {text}", spec.key);
    }
    assert!(text.contains("http://host:8080"), "{text}");
    assert!(
        text.contains("(experimental)"),
        "unset shows default: {text}"
    );
    assert!(
        text.contains("proxy · default: direct connection"),
        "{text}"
    );
    assert!(text.contains("[x] back to the default"), "{text}");
}

/// The prompt shows the typed text; a confirm shows its question.
#[test]
fn settings_prompt_and_confirm_render() {
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    view.selected = key_index("proxy");
    view.input = Some("http://new:1".to_string());
    let text = settings_screen(&view, 110, 26);
    assert!(text.contains("http://new:1_"), "{text}");
    assert!(text.contains("empty unsets it"), "{text}");

    view.input = None;
    view.confirm = Some(Pending {
        index: key_index("secret_store"),
        value: Some("memory".to_string()),
    });
    let text = settings_screen(&view, 110, 26);
    assert!(
        text.contains("Change where the session tokens are kept?"),
        "{text}"
    );
    assert!(text.contains("[y] yes"), "{text}");
}

/// The highlight moves within the keys and stays on screen.
#[test]
fn settings_highlight_stays_visible() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.settings = SettingsView::with_values(vec![None; SPECS.len()]);
    browser.settings_move(100);
    assert_eq!(browser.settings.selected, SPECS.len() - 1);
    let text = settings_screen(&browser.settings, 110, 12);
    assert!(text.contains("Session tokens"), "{text}");
    browser.settings_move(-100);
    assert_eq!(browser.settings.selected, 0);
}

/// The Profiles tab lists the profiles (default marked) beside the
/// highlighted one's file, games, issues and TOML.
#[test]
fn settings_profiles_tab_renders() {
    use gaki_tui::app::browser::settings::{ProfilesTab, Tab};
    use gaki_tui::app::steam::{ProfileRow, Profiles};
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    view.tab = Tab::Profiles;
    let mut tab = ProfilesTab::with(Profiles {
        dir: "/cfg/profiles".into(),
        rows: vec![
            ProfileRow {
                name: "proton".into(),
                path: "/cfg/profiles/proton.toml".into(),
                is_default: true,
                games: vec!["portal (400)".into()],
                issues: Vec::new(),
                toml: "name = \"proton\"\nrunner = \"proton\"\n".into(),
            },
            ProfileRow {
                name: "wine".into(),
                path: "/cfg/profiles/wine.toml".into(),
                issues: vec!["error: profile wine: no runner".into()],
                ..Default::default()
            },
        ],
        errors: vec!["broken.toml: bad syntax".into()],
    });
    let text = settings_screen(
        &{
            view.profiles_tab = Some(tab.clone());
            view.clone()
        },
        110,
        26,
    );
    assert!(text.contains("Profiles"), "{text}");
    assert!(text.contains("proton  default"), "{text}");
    assert!(text.contains("/cfg/profiles/proton.toml"), "{text}");
    assert!(text.contains("portal (400)"), "{text}");
    assert!(text.contains("runner = \"proton\""), "{text}");
    assert!(text.contains("broken.toml: bad syntax"), "{text}");
    assert!(text.contains("[d] make default"), "{text}");

    tab.selected = 1;
    view.profiles_tab = Some(tab);
    let text = settings_screen(&view, 110, 26);
    assert!(text.contains("no runner"), "{text}");
    assert!(text.contains("no game names it"), "{text}");
}

/// The Check tab shows the findings, or that there are none.
#[test]
fn settings_check_tab_renders() {
    use gaki_tui::app::browser::settings::{CheckTab, Tab};
    use gaki_tui::app::steam::CheckFinding;
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    view.tab = Tab::Check;
    view.check = Some(CheckTab::with(Vec::new()));
    assert!(settings_screen(&view, 100, 16).contains("No problems found."));

    view.check = Some(CheckTab::with(vec![
        CheckFinding {
            error: true,
            text: "error: config.toml: bad".into(),
        },
        CheckFinding {
            error: false,
            text: "warning: scripts/1.sh: old".into(),
        },
    ]));
    let text = settings_screen(&view, 100, 16);
    assert!(text.contains("error: config.toml: bad"), "{text}");
    assert!(text.contains("warning: scripts/1.sh: old"), "{text}");
    assert!(text.contains("[r] check again"), "{text}");
}

/// Tab and Shift-Tab cycle the tabs; the check runs and the daemons are
/// listed when their tabs are shown.
#[test]
fn settings_tabs_cycle() {
    use gaki_tui::app::browser::settings::Tab;
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true)]);
    browser.settings = SettingsView::with_values(vec![None; SPECS.len()]);
    browser.settings_tab(true);
    assert_eq!(browser.settings.tab, Tab::Profiles);
    assert!(browser.settings.profiles_tab.is_some(), "profiles loaded");
    browser.settings_tab(true);
    assert_eq!(browser.settings.tab, Tab::Check);
    assert!(browser.settings.check.is_some(), "check ran");
    browser.settings_tab(true);
    assert_eq!(browser.settings.tab, Tab::Daemons);
    assert!(browser.settings.daemons.is_some(), "daemons listed");
    browser.settings_tab(true);
    assert_eq!(browser.settings.tab, Tab::Clean);
    assert!(browser.settings.clean.is_some(), "clean scan started");
    browser.settings_tab(true);
    assert_eq!(browser.settings.tab, Tab::Settings);
    browser.settings_tab(false);
    assert_eq!(browser.settings.tab, Tab::Clean);
}

#[test]
fn every_game_links_to_steam_protondb_steamdb_and_the_proton_tracker() {
    isolate_config();
    for installed in [true, false] {
        let browser = Browser::new(vec![game(1, "Alpha", installed)]);
        let rows = browser.action_candidates();
        for (action, key) in [
            (Action::SteamPage, "E"),
            (Action::ProtonDb, "B"),
            (Action::ProtonIssues, "T"),
            (Action::SteamDb, "Z"),
        ] {
            let row = rows.iter().find(|r| r.action == action).expect("link row");
            assert_eq!(row.category, "Links");
            assert_eq!(row.key(), key);
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
    assert_eq!(
        Link::SteamDb.url(945360),
        "https://steamdb.info/app/945360/"
    );
    assert_eq!(
        Link::SteamDbDepot.url(945361),
        "https://steamdb.info/depot/945361/manifests/"
    );
}

fn daemon(pid: u32, config_dir: Option<&str>) -> libgaki::ops::daemon::GakiProcess {
    libgaki::ops::daemon::GakiProcess {
        pid,
        is_daemon: true,
        command: format!("gaki-cli --config-dir {} daemon", config_dir.unwrap_or("-")),
        config_dir: config_dir.map(str::to_string),
        socket: None,
    }
}

#[test]
fn settings_daemons_tab_lists_and_confirms() {
    use gaki_tui::app::browser::settings::{DaemonsTab, StopConfirm, Tab};
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    view.tab = Tab::Daemons;
    view.daemons = Some(DaemonsTab::with(vec![
        daemon(4242, None),
        daemon(4300, Some("/home/me/.config/gaki-test")),
    ]));
    let screen = settings_screen(&view, 120, 24);
    assert!(screen.contains(" Daemons "), "{screen}");
    assert!(screen.contains("▶ 4242"), "{screen}");
    assert!(screen.contains("/home/me/.config/gaki-test"), "{screen}");
    assert!(screen.contains("(default)"), "{screen}");
    assert!(
        screen.contains("gaki-cli --config-dir - daemon"),
        "{screen}"
    );
    assert!(screen.contains("[S] stop all"), "{screen}");

    view.daemons.as_mut().unwrap().confirm = Some(StopConfirm {
        pids: vec![4242, 4300],
        all: true,
    });
    let screen = settings_screen(&view, 120, 24);
    assert!(screen.contains("Stop all 2 daemons?"), "{screen}");
    assert!(screen.contains("[y] yes"), "{screen}");
}

#[test]
fn settings_daemons_tab_says_when_none_run() {
    use gaki_tui::app::browser::settings::{DaemonsTab, Tab};
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    view.tab = Tab::Daemons;
    view.daemons = Some(DaemonsTab::with(Vec::new()));
    let screen = settings_screen(&view, 100, 20);
    assert!(
        screen.contains("No gaki-cli daemon is running."),
        "{screen}"
    );
}

/// The Clean tab lists each kind with its size and a total, and asks
/// before deleting.
#[test]
fn clean_tab_lists_kinds_and_asks() {
    use libgaki::ops::clean::{Item, Kind};
    let item = |kind, bytes| Item {
        kind,
        path: "/nonexistent".into(),
        bytes,
        keep_dir: false,
        app_id: None,
    };
    let mut view = SettingsView::with_values(vec![None; SPECS.len()]);
    view.tab = Tab::Clean;
    view.clean = Some(CleanTab::with(vec![
        item(Kind::Images, 2048),
        item(Kind::Shaders, 1024),
        item(Kind::Shaders, 1024),
    ]));
    let text = settings_screen(&view, 100, 20);
    assert!(text.contains("images"), "{text}");
    assert!(text.contains("2.00 KiB  2 item(s)"), "{text}");
    assert!(text.contains("4.00 KiB"), "{text}");
    assert!(text.contains("[x] delete"), "{text}");

    view.clean.as_mut().unwrap().confirm = true;
    let text = settings_screen(&view, 100, 20);
    assert!(text.contains("Delete all of this (4.00 KiB)?"), "{text}");
}
