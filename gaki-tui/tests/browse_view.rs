//! Exercises the browse model (tab filters + fuzzy query + sort) and renders the
//! browse widgets into tui's off-screen `TestBackend` to prove they lay out and
//! paint the expected content.

use gaki_tui::app::browser::{Browser, Filter};
use gaki_tui::app::game::Game;
#[cfg(feature = "workshop")]
use gaki_tui::app::steam::WorkshopItem;
use gaki_tui::app::steam::{LibraryGame, ProtonRuntime, WorkshopComment};
use gaki_tui::ui::{self, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Point the config at a throwaway dir so tests don't read/write the real one.
fn isolate_config() {
    let dir = std::env::temp_dir().join("gaki-tui-test-config");
    // A repeat call in the same process keeps the first dir, which is fine.
    libgaki::core::config::set_config_dir(dir.clone());
    // SAFETY: set before any Config::cached() call in this test process.
    unsafe {
        std::env::set_var("XDG_CACHE_HOME", &dir);
    }
}

fn game(id: u32, name: &str, installed: bool, update: bool) -> Game {
    Game::from_library(LibraryGame {
        app_id: id,
        name: name.to_string(),
        is_installed: installed,
        install_path: None,
        update_available: update,
        is_owned: true,
        is_family_shared: false,
        platform: None,
        active_branch: None,
        assets: None,
        store_url: None,
    })
}

#[test]
fn filter_and_query_narrow_the_view() {
    isolate_config();
    let games = vec![
        game(1, "Alpha", true, false),
        game(2, "Beta", false, false),
        game(3, "Gamma", true, true),
    ];
    let mut browser = Browser::new(games);
    assert_eq!(browser.visible().len(), 3, "All shows everything");

    browser.set_filter(Filter::Installed);
    assert_eq!(browser.visible().len(), 2, "Installed: Alpha + Gamma");

    browser.set_filter(Filter::Updates);
    assert_eq!(browser.visible().len(), 1, "Updates: only Gamma");

    browser.set_filter(Filter::All);
    browser.push_query('l'); // fuzzy-matches "Alpha"
    let names: Vec<String> = browser
        .visible()
        .iter()
        .map(|g| g.raw_name().to_string())
        .collect();
    assert!(names.contains(&"Alpha".to_string()));
    assert!(!names.contains(&"Beta".to_string()));
}

#[test]
fn counts_reflect_library() {
    isolate_config();
    let browser = Browser::new(vec![
        game(1, "Alpha", true, false),
        game(2, "Beta", false, false),
        game(3, "Gamma", true, true),
    ]);
    let c = browser.counts();
    assert_eq!(c.total, 3);
    assert_eq!(c.installed, 2);
    assert_eq!(c.updates, 1);
}

#[test]
fn browse_widgets_render_without_panicking() {
    isolate_config();
    let mut browser = Browser::new(vec![
        game(1, "Alpha", true, false),
        game(2, "Beta", false, true),
    ]);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            f.render_widget(ui::tabs::library_tabs(&browser), Rect::new(0, 0, 60, 3));
            #[cfg(feature = "social")]
            f.render_widget(ui::tabs::friends_tabs(&browser), Rect::new(60, 0, 20, 3));
            let list = ui::list::list(&browser, 40, 0);
            f.render_stateful_widget(list, Rect::new(0, 3, 40, 18), &mut browser.state);
            let selected = browser.selected();
            f.render_widget(
                ui::detail::detail(selected.as_ref(), false, 30, 16),
                Rect::new(40, 3, 40, 18),
            );
            f.render_widget(
                ui::status::status_bar(&browser, Some("tester")),
                Rect::new(0, 22, 80, 2),
            );
        })
        .unwrap();

    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();

    assert!(text.contains("Alpha"), "list shows a game name");
    assert!(text.contains("tester"), "status bar shows the account");
    assert!(text.contains("installed"), "status bar shows counts");
    assert!(text.contains("Detail"), "detail panel renders");
}

#[cfg(feature = "install")]
#[test]
fn install_picker_lists_libraries_and_marks_choice() {
    isolate_config();
    let mut browser = Browser::new(vec![]);
    browser.install_libraries = vec![
        gaki_tui::app::steam::LibraryFolder {
            path: "C:\\SteamLibrary".to_string(),
            free_bytes: Some(2_000_000_000),
        },
        gaki_tui::app::steam::LibraryFolder {
            path: "D:\\Games\\SteamLibrary".to_string(),
            free_bytes: Some(80_000_000_000),
        },
    ];
    browser.install_estimate = Some(50_000_000_000); // ~50 GB: too big for C:, fits D:
    browser.install_picker_index = 1;

    let backend = TestBackend::new(70, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(
                ui::install_picker::install_picker(&browser),
                Rect::new(0, 0, 70, 10),
            );
        })
        .unwrap();

    let rows: Vec<String> = (0..10)
        .map(|y| {
            (0..70)
                .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect();
    let joined = rows.join("\n");

    assert!(joined.contains("which library"), "picker title shown");
    assert!(joined.contains("C:\\SteamLibrary"), "first library listed");
    assert!(
        joined.contains("D:\\Games\\SteamLibrary"),
        "second library listed"
    );
    // The selection marker sits on the highlighted (second) row.
    let marked = rows
        .iter()
        .find(|r| r.contains('\u{25b6}'))
        .expect("a marked row");
    assert!(
        marked.contains("D:\\Games\\SteamLibrary"),
        "the marker is on the chosen library, got: {marked}"
    );
    // The estimate header and per-drive free space are shown.
    assert!(joined.contains("on disk"), "estimated install size shown");
    assert!(joined.contains("free"), "free space shown per library");
    // C: (2 GB free) can't fit the ~50 GB estimate -> marked; D: (80 GB) can.
    let c_row = rows
        .iter()
        .find(|r| r.contains("C:\\SteamLibrary"))
        .unwrap();
    assert!(
        c_row.contains('\u{2717}'),
        "the too-small drive is marked, got: {c_row}"
    );
    let d_row = rows
        .iter()
        .find(|r| r.contains("D:\\Games\\SteamLibrary"))
        .unwrap();
    assert!(
        !d_row.contains('\u{2717}'),
        "the fitting drive is not marked, got: {d_row}"
    );
}

/// Render the complete browse frame the way `main` lays it out — tabs / [list |
/// cover+detail] / status — with a game that has no description (so the right
/// pane has only two chunks). Regression for an index-out-of-bounds panic where
/// the status bar read the wrong layout.
#[test]
fn full_browse_frame_renders() {
    isolate_config();
    let mut browser = Browser::new(vec![
        game(1, "Alpha", true, false),
        game(2, "Beta", false, true),
    ]);
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(2),
                    Constraint::Length(2),
                ])
                .split(f.area());
            let tab_areas = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(20), Constraint::Length(19)])
                .split(chunks[0]);
            f.render_widget(ui::tabs::library_tabs(&browser), tab_areas[0]);
            #[cfg(feature = "social")]
            f.render_widget(ui::tabs::friends_tabs(&browser), tab_areas[1]);

            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(chunks[1]);
            let list = ui::list::list(&browser, body[0].width, 0);
            f.render_stateful_widget(list, body[0], &mut browser.state);

            let selected = browser.selected();
            // No description in tests → right pane is cover + table (two chunks).
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Min(0), Constraint::Length(13)])
                .split(body[1]);
            f.render_widget(theme::panel("Cover".to_string()), right_chunks[0]);
            f.render_widget(
                ui::detail::detail(selected.as_ref(), false, 38, 11),
                right_chunks[1],
            );

            // Status bar at the OUTER bottom chunk — the regression point.
            f.render_widget(ui::status::status_bar(&browser, Some("tester")), chunks[2]);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let bottom: String = (28..30)
        .flat_map(|y| (0..80u16).map(move |x| (x, y)))
        .map(|(x, y)| buf.cell((x, y)).unwrap().symbol())
        .collect();
    assert!(
        bottom.contains("tester"),
        "status bar renders at the bottom"
    );
}

/// The Steam Cloud overlay advertises the sync directions in its title so the
/// directional keys (`d` down-only, `u` up-only) are discoverable next to the
/// plain `s` sync.
#[cfg(feature = "cloud")]
#[test]
fn cloud_overlay_shows_directional_hints() {
    isolate_config();
    let browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            f.render_widget(ui::cloud::cloud(&browser), Rect::new(0, 0, 80, 12));
        })
        .unwrap();

    let title: String = (0..80)
        .map(|x| terminal.backend().buffer().cell((x, 0)).unwrap().symbol())
        .collect();
    assert!(
        title.contains("[s] sync"),
        "title keeps the both-ways sync hint"
    );
    assert!(title.contains("[d] down"), "title advertises download-only");
    assert!(title.contains("[u] up"), "title advertises upload-only");
}

/// The cover panel and the detail panel must occupy separate, ordered rows so
/// the artwork never overlaps the details (regression for the overlap fix).
#[test]
fn cover_and_detail_do_not_overlap() {
    isolate_config();
    let browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    let backend = TestBackend::new(60, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    // A fixed test layout: a Cover panel (top 40%) over Detail (bottom 60%).
    let right = Rect::new(0, 0, 60, 24);
    terminal
        .draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(right);
            f.render_widget(theme::panel("Cover".to_string()), chunks[0]);
            f.render_widget(
                ui::detail::detail(browser.selected().as_ref(), false, 40, 12),
                chunks[1],
            );
        })
        .unwrap();

    // Find which row each panel title lands on.
    let buf = terminal.backend().buffer();
    let row_text = |y: u16| -> String {
        (0..60)
            .map(|x| buf.cell((x, y)).unwrap().symbol())
            .collect()
    };
    let cover_row = (0..24).find(|&y| row_text(y).contains("Cover"));
    let detail_row = (0..24).find(|&y| row_text(y).contains("Detail"));

    let cover_row = cover_row.expect("Cover title rendered");
    let detail_row = detail_row.expect("Detail title rendered");
    assert!(
        cover_row < detail_row,
        "Cover panel must sit above Detail (cover row {cover_row}, detail row {detail_row})"
    );
    // The detail panel must start at or below the cover panel's bottom border.
    assert!(
        detail_row as i32 - cover_row as i32 >= 8,
        "Cover and Detail occupy clearly separate bands"
    );
}

fn proton(name: &str, official: bool, installed: bool) -> ProtonRuntime {
    ProtonRuntime {
        name: name.to_string(),
        official,
        installed,
        is_default: false,
    }
}

/// Render the whole proton overlay into a wide buffer and return every cell as
/// one string, so we can assert on the title hints regardless of row.
fn render_proton(browser: &Browser) -> String {
    let backend = TestBackend::new(90, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| f.render_widget(ui::proton::proton(browser), f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..16)
        .map(|y| {
            (0..90)
                .map(|x| buf.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Only an installed official runtime is uninstallable (Steam removes it;
/// custom builds belong to the package manager). The overlay surfaces the [u]
/// uninstall hint exactly then, and always offers [i] install.
#[cfg(feature = "install")]
#[test]
fn proton_overlay_offers_install_and_guards_uninstall() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.protons = vec![
        proton("GE-Proton10-34", false, true), // installed custom: not removable
        proton("Proton 9.0", true, false),     // official, not installed
        proton("Proton 10.0", true, true),     // installed official: removable
    ];

    // Highlight the custom runtime: install is always offered, uninstall is not.
    browser.proton_index = 0;
    assert!(!browser.selected_proton_uninstallable());
    let custom = render_proton(&browser);
    assert!(
        custom.contains("[i] install"),
        "install hint always present"
    );
    assert!(
        !custom.contains("[u] uninstall"),
        "custom runtime must not offer uninstall"
    );

    // Highlight the installed official runtime: uninstall is offered.
    browser.proton_index = 2;
    assert!(browser.selected_proton_uninstallable());
    assert!(
        render_proton(&browser).contains("[u] uninstall"),
        "installed official runtime offers uninstall"
    );

    // A not-installed runtime is never uninstallable.
    browser.proton_index = 1;
    assert!(!browser.selected_proton_uninstallable());
}

/// Without `install` the overlay only sets the default.
#[cfg(not(feature = "install"))]
#[test]
fn proton_overlay_without_installs_only_sets_the_default() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.protons = vec![proton("Proton 10.0", true, true)];
    let text = render_proton(&browser);
    assert!(text.contains("[d] default"), "{text}");
    assert!(!text.contains("[i] install"), "{text}");
    assert!(!text.contains("[u] uninstall"), "{text}");
}

/// An in-flight install streams a status line into the overlay title.
#[test]
fn proton_overlay_shows_install_status() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.protons = vec![proton("Proton 9.0", true, false)];
    *browser.proton_status.lock().unwrap() = Some(gaki_tui::app::game::status::GameStatus::msg(
        &None,
        "downloading 42.0%",
    ));
    assert!(
        render_proton(&browser).contains("downloading 42.0%"),
        "streamed install status appears in the overlay"
    );
}

#[cfg(feature = "workshop")]
fn workshop_item(id: u64, title: &str, subscribed: bool) -> WorkshopItem {
    WorkshopItem {
        id: Some(id),
        title: title.to_string(),
        subscribed,
        ..Default::default()
    }
}

/// Leaving browse mode and closing the overlay reset the browse state. (We set
/// the flags directly rather than via `open_workshop`/`workshop_enter_browse`,
/// which would reach Steam.)
#[cfg(feature = "workshop")]
#[test]
fn workshop_browse_mode_toggles() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.show_workshop = true;
    browser.workshop_browse = true;
    browser.workshop_query = "x".to_string();

    browser.workshop_exit_browse();
    assert!(!browser.workshop_browse, "back to the subscribed list");

    browser.close_workshop();
    assert!(!browser.show_workshop, "overlay closed");
    assert!(!browser.workshop_browse, "browse state reset on close");
    assert!(browser.workshop_query.is_empty(), "query cleared on close");
}

/// Highlight navigation over the browse results is clamped at both ends.
#[cfg(feature = "workshop")]
#[test]
fn workshop_result_navigation_clamps() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.show_workshop = true;
    browser.workshop_browse = true;
    browser.workshop_results = vec![
        workshop_item(10, "Mod A", false),
        workshop_item(20, "Mod B", true),
    ];
    browser.workshop_index = 0;

    browser.workshop_previous();
    assert_eq!(browser.workshop_index, 0, "cannot go above the first row");

    browser.workshop_next();
    assert_eq!(browser.workshop_index, 1, "moved to the second row");
    browser.workshop_next();
    assert_eq!(browser.workshop_index, 1, "cannot go past the last row");

    let sel = browser
        .selected_workshop_result()
        .expect("a row is selected");
    assert_eq!(sel.id, Some(20));
}

/// The browse pane renders the query, a result count, and the rows with their
/// subscribed tags and the highlight marker.
#[cfg(feature = "workshop")]
#[test]
fn workshop_browse_pane_renders() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.show_workshop = true;
    browser.workshop_browse = true;
    browser.workshop_query = "aim".to_string();
    browser.workshop_results = vec![
        workshop_item(10, "AimTrainer", false),
        workshop_item(20, "BotMap", true),
    ];
    browser.workshop_index = 1;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(ui::workshop::workshop(&browser), Rect::new(0, 0, 80, 24));
        })
        .unwrap();

    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();

    assert!(text.contains("aim"), "query text is shown");
    assert!(text.contains("AimTrainer"), "first result title shown");
    assert!(text.contains("BotMap"), "second result title shown");
    assert!(text.contains("subscribed"), "subscribed tag shown");
    assert!(text.contains("browse"), "browse-mode title shown");
}

/// Opening the comments sub-pane and exiting browse both reset its state. We set
/// the flags directly rather than via `workshop_open_comments` (which would
/// reach Steam), to keep the test offline.
#[cfg(feature = "workshop")]
#[test]
fn workshop_comments_state_resets() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.show_workshop = true;
    browser.workshop_browse = true;
    browser.workshop_comments_open = true;
    browser.workshop_comments = vec![WorkshopComment {
        author: "Ada".to_string(),
        message: "great mod".to_string(),
        ..Default::default()
    }];
    browser.workshop_comments_scroll = 0;

    // Scroll is clamped to the single row (cannot advance past the last).
    browser.workshop_comments_scroll_down();
    assert_eq!(
        browser.workshop_comments_scroll, 0,
        "scroll clamps on a single comment"
    );

    browser.close_workshop_comments();
    assert!(!browser.workshop_comments_open, "sub-pane closed and reset");
    assert!(
        browser.workshop_comments.is_empty(),
        "comments dropped on close"
    );

    // Exiting browse also tears the sub-pane down.
    browser.workshop_comments_open = true;
    browser.workshop_exit_browse();
    assert!(
        !browser.workshop_comments_open,
        "leaving browse closes the comments sub-pane"
    );
}

/// The comments sub-pane renders the item id in the title, a comment count, and
/// each comment's author header and body.
#[cfg(feature = "workshop")]
#[test]
fn workshop_comments_pane_renders() {
    isolate_config();
    let mut browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    browser.show_workshop = true;
    browser.workshop_browse = true;
    browser.workshop_comments_open = true;
    browser.workshop_comments = vec![
        WorkshopComment {
            author: "Ada".to_string(),
            message: "great mod".to_string(),
            ..Default::default()
        },
        WorkshopComment {
            author: "Grace".to_string(),
            message: "crashes on load".to_string(),
            ..Default::default()
        },
    ];

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(ui::workshop::workshop(&browser), Rect::new(0, 0, 80, 24));
        })
        .unwrap();

    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();

    assert!(text.contains("Comments"), "comments-pane title shown");
    assert!(text.contains("Ada"), "first author shown");
    assert!(text.contains("great mod"), "first comment body shown");
    assert!(text.contains("Grace"), "second author shown");
    assert!(text.contains("2 comment"), "comment count shown");
}

/// A comment without an author shows a placeholder.
#[test]
fn workshop_comment_without_author_is_anonymous() {
    let anon = WorkshopComment {
        message: "no name".into(),
        ..Default::default()
    };
    assert_eq!(anon.display_author(), "(anonymous)");
    let named = WorkshopComment {
        author: "76561197960287930".into(),
        ..Default::default()
    };
    assert_eq!(named.display_author(), "76561197960287930");
}

#[test]
fn version_badge_sits_in_bottom_right_corner() {
    isolate_config();
    let browser = Browser::new(vec![game(1, "Alpha", true, false)]);
    let badge = format!("gaki-tui v{}", env!("CARGO_PKG_VERSION"));
    let render = |width: u16| -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, 2)).unwrap();
        let area = Rect::new(0, 0, width, 2);
        terminal
            .draw(|f| {
                f.render_widget(ui::status::status_bar(&browser, None), area);
                f.render_widget(
                    ui::status::version_badge(),
                    ui::status::version_badge_area(area),
                );
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        (0..2)
            .map(|y| {
                (0..width)
                    .map(|x| buf.cell((x, y)).unwrap().symbol())
                    .collect()
            })
            .collect()
    };

    let rows = render(140);
    assert!(
        rows[1].trim_end().ends_with(&badge),
        "badge is bottom-right"
    );
    assert!(rows[1].contains("[q] quit"), "key hints are not covered");

    // Too narrow for both on the bottom row: the badge moves up, hints stay whole.
    // 105 is that in every build (the hints are 104 wide with `install`, 91
    // without).
    let rows = render(105);
    assert!(rows[0].trim_end().ends_with(&badge));
    assert!(rows[1].contains("[q] quit"));
}
