//! Renders the Installs overlay (`X`) into an off-screen `TestBackend`.
#![cfg(feature = "install")]

use gaki_tui::app::browser::installs::{ForgetConfirm, InstallsView};
use gaki_tui::ui;
use libgaki::ops::install::{InstallRow, RUNNING_ELSEWHERE};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render(view: &InstallsView, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::installs::draw(frame, frame.area(), view))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn row(name: &str, state: &'static str, done: u64, total: u64, status: &str) -> InstallRow {
    InstallRow {
        app_id: 1,
        name: name.into(),
        total_bytes: total,
        downloaded_bytes: done,
        status: status.into(),
        is_downloading: state == "running",
        state,
        entry: None,
    }
}

#[test]
fn rows_show_state_progress_and_reason() {
    let gib = 1024 * 1024 * 1024;
    let view = InstallsView::with_rows(vec![
        row("Portal 2", "running", 4 * gib, 8 * gib, "downloading"),
        row("Hades", "interrupted", 0, 0, "disk full"),
        row("Celeste", RUNNING_ELSEWHERE, 0, 0, ""),
    ]);
    let screen = render(&view, 120, 14);
    assert!(screen.contains("Installs (3)"), "{screen}");
    assert!(screen.contains("▶ Portal 2"), "{screen}");
    assert!(screen.contains("50%"), "{screen}");
    assert!(screen.contains("interrupted"), "{screen}");
    assert!(screen.contains("disk full"), "{screen}");
    assert!(screen.contains("running elsewhere"), "{screen}");
    assert!(screen.contains("[R] resume all"), "{screen}");
}

#[test]
fn an_empty_list_says_so() {
    let screen = render(&InstallsView::with_rows(Vec::new()), 90, 8);
    assert!(screen.contains("the backlog is empty"), "{screen}");
}

#[test]
fn forget_with_files_warns() {
    let mut view = InstallsView::with_rows(vec![row("Hades", "paused", 0, 0, "")]);
    view.confirm = Some(ForgetConfirm {
        app_id: 1,
        name: "Hades".into(),
        delete_files: true,
    });
    let screen = render(&view, 110, 8);
    assert!(
        screen.contains("Forget Hades and delete its partial files?"),
        "{screen}"
    );
    assert!(screen.contains("[y] yes  [n] no"), "{screen}");
}

#[test]
fn the_duplicates_tab_lists_keep_and_stale_copies() {
    use gaki_tui::app::browser::installs::{DuplicatesTab, InstallsTab};
    use gaki_tui::app::steam::{DuplicateRow, StaleCopy};
    let gib = 1024 * 1024 * 1024;
    let mut view = InstallsView::with_rows(Vec::new());
    view.tab = InstallsTab::Duplicates;
    view.duplicates = Some(DuplicatesTab::with(vec![DuplicateRow {
        app_id: 620,
        name: "Portal 2".into(),
        keep: "/games/SteamLibrary/steamapps/common/Portal 2".into(),
        keep_updated: 1_700_000_000,
        stale: vec![StaleCopy {
            path: "/old/steamapps/common/Portal 2".into(),
            last_updated: 1_600_000_000,
            size: 12 * gib,
        }],
    }]));
    let screen = render(&view, 130, 16);
    assert!(screen.contains("[Duplicates (1)]"), "{screen}");
    assert!(screen.contains("▶ Portal 2"), "{screen}");
    assert!(
        screen.contains("keep   /games/SteamLibrary/steamapps/common/Portal 2"),
        "{screen}"
    );
    assert!(
        screen.contains("stale  /old/steamapps/common/Portal 2"),
        "{screen}"
    );
    assert!(screen.contains("2023-11-14"), "{screen}");
    assert!(screen.contains("reclaimable"), "{screen}");
    assert!(screen.contains("gaki never deletes a copy"), "{screen}");
}

#[test]
fn the_duplicates_tab_says_when_there_are_none() {
    use gaki_tui::app::browser::installs::{DuplicatesTab, InstallsTab};
    let mut view = InstallsView::with_rows(Vec::new());
    view.tab = InstallsTab::Duplicates;
    view.duplicates = Some(DuplicatesTab::with(Vec::new()));
    let screen = render(&view, 100, 10);
    assert!(
        screen.contains("No game is installed in more than one Steam library."),
        "{screen}"
    );
}
