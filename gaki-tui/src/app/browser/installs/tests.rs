use super::*;
use libgaki::library::backlog::{BacklogEntry, BacklogState};

fn entry(app_id: u32, state: BacklogState) -> BacklogEntry {
    BacklogEntry {
        app_id,
        name: format!("Game {app_id}"),
        state,
        platform: "windows".into(),
        library: "/games".into(),
        install_dir: None,
        is_dlc: false,
        restart_steam: false,
        downloaded_bytes: 0,
        total_bytes: 0,
        reason: None,
        error_type: None,
        auto_attempts: 0,
        added_at: 0,
        updated_at: 0,
    }
}

fn row(app_id: u32, state: &'static str) -> InstallRow {
    InstallRow {
        app_id,
        name: format!("Game {app_id}"),
        total_bytes: 100,
        downloaded_bytes: 50,
        status: String::new(),
        is_downloading: state == "running",
        state,
        entry: (state != "running").then(|| entry(app_id, BacklogState::Paused)),
    }
}

fn browser_with(rows: Vec<InstallRow>) -> Browser {
    let mut browser = Browser::new(vec![]);
    browser.installs = Some(InstallsView::with_rows(rows));
    browser
}

fn view(browser: &Browser) -> &InstallsView {
    browser.installs.as_ref().unwrap()
}

#[test]
fn row_kinds() {
    assert_eq!(row_kind(&row(1, "running")), RowKind::Running);
    assert_eq!(row_kind(&row(1, RUNNING_ELSEWHERE)), RowKind::Elsewhere);
    assert_eq!(row_kind(&row(1, "paused")), RowKind::Waiting);
    assert_eq!(row_kind(&row(1, "interrupted")), RowKind::Waiting);
}

#[test]
fn moving_clamps() {
    let mut browser = browser_with(vec![row(1, "running"), row(2, "paused")]);
    browser.installs_move(5);
    assert_eq!(view(&browser).selected, 1);
    browser.installs_move(-5);
    assert_eq!(view(&browser).selected, 0);
}

#[test]
fn shrinking_rows_clamp_the_highlight() {
    let mut browser = browser_with(vec![row(1, "paused"), row(2, "paused")]);
    browser.installs_move(1);
    let installs = browser.installs.as_mut().unwrap();
    installs.set_rows(Ok(vec![row(1, "paused")]));
    assert_eq!(installs.selected, 0);
}

#[test]
fn forgetting_asks_first_and_never_for_running_installs() {
    let mut browser = browser_with(vec![
        row(1, "running"),
        row(2, RUNNING_ELSEWHERE),
        row(3, "paused"),
    ]);
    browser.begin_installs_forget(false);
    assert!(!browser.installs_confirm_open());
    assert_eq!(view(&browser).status, "Stop it first.");

    browser.installs_move(1);
    browser.begin_installs_forget(true);
    assert!(!browser.installs_confirm_open());
    assert!(view(&browser).status.contains("Another gaki process"));

    browser.installs_move(1);
    browser.begin_installs_forget(true);
    let confirm = view(&browser).confirm.clone().unwrap();
    assert_eq!(confirm.app_id, 3);
    assert!(confirm.delete_files);
    // "No" drops the question and changes nothing.
    browser.answer_installs_confirm(false);
    assert!(!browser.installs_confirm_open());
    assert_eq!(view(&browser).rows.as_ref().unwrap().len(), 3);
}

#[test]
fn closing_drops_the_view() {
    let mut browser = browser_with(vec![]);
    assert!(browser.overlay_open());
    browser.close_installs();
    assert!(browser.installs.is_none());
}

fn duplicate(app_id: u32) -> steam::DuplicateRow {
    steam::DuplicateRow {
        app_id,
        name: format!("app {app_id}"),
        keep: "/a".into(),
        keep_updated: 0,
        stale: vec![steam::StaleCopy {
            path: "/b".into(),
            last_updated: 0,
            size: 10,
        }],
    }
}

#[test]
fn tab_switches_between_the_queue_and_duplicates() {
    let mut browser = browser_with(vec![row(1, "paused"), row(2, "paused")]);
    // A finished scan, so switching doesn't start a real one.
    browser.installs.as_mut().unwrap().duplicates = Some(DuplicatesTab::with(vec![
        duplicate(1),
        duplicate(2),
        duplicate(3),
    ]));
    browser.installs_tab();
    assert_eq!(view(&browser).tab, InstallsTab::Duplicates);

    // j/k move the duplicates highlight, not the queue's.
    browser.installs_move(10);
    assert_eq!(view(&browser).duplicates.as_ref().unwrap().selected, 2);
    assert_eq!(view(&browser).selected, 0);

    browser.installs_tab();
    assert_eq!(view(&browser).tab, InstallsTab::Queue);
    assert_eq!(view(&browser).duplicates.as_ref().unwrap().selected, 2);
}

#[test]
fn reclaimable_sums_the_stale_copies() {
    let mut row = duplicate(1);
    row.stale.push(steam::StaleCopy {
        path: "/c".into(),
        last_updated: 0,
        size: 5,
    });
    assert_eq!(row.reclaimable(), 15);
}
