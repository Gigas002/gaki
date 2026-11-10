use super::transfer::{DEFAULT_PATH, PlanLine, plan_lines};
use super::*;
use libgaki::library::backlog::{BacklogEntry, BacklogState};
use libgaki::library::transfer::{ImportPlan, LibraryEntry};

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
    browser.installs_tab(true);
    assert_eq!(view(&browser).tab, InstallsTab::Duplicates);

    // j/k move the duplicates highlight, not the queue's.
    browser.installs_move(10);
    assert_eq!(view(&browser).duplicates.as_ref().unwrap().selected, 2);
    assert_eq!(view(&browser).selected, 0);

    browser.installs_tab(false);
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

fn lib_entry(app_id: u32, branch: Option<&str>) -> LibraryEntry {
    LibraryEntry {
        app_id,
        name: format!("Game {app_id}"),
        platform: None,
        branch: branch.map(str::to_string),
    }
}

fn on_transfer() -> Browser {
    let mut browser = browser_with(vec![]);
    browser.installs_tab(false);
    assert_eq!(view(&browser).tab, InstallsTab::Transfer);
    browser
}

/// Poll the Transfer tab until its jobs finish.
fn settle(browser: &mut Browser) {
    for _ in 0..200 {
        let tab = &mut browser.installs.as_mut().unwrap().transfer;
        tab.poll();
        if !tab.busy() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the transfer job didn't finish");
}

#[test]
fn tabs_cycle_through_all_three_both_ways() {
    let mut tab = InstallsTab::Queue;
    for expected in [
        InstallsTab::Duplicates,
        InstallsTab::Transfer,
        InstallsTab::Queue,
    ] {
        tab = tab.step(true);
        assert_eq!(tab, expected);
    }
    assert_eq!(InstallsTab::Queue.step(false), InstallsTab::Transfer);
}

#[test]
fn the_path_is_typed_and_falls_back_to_the_default_when_cleared() {
    let mut browser = on_transfer();
    browser.transfer_edit_path();
    assert!(browser.transfer_editing());
    for _ in 0..DEFAULT_PATH.len() {
        browser.transfer_path_pop();
    }
    browser.transfer_path_done();
    assert_eq!(view(&browser).transfer.path, DEFAULT_PATH);

    browser.transfer_edit_path();
    for _ in 0..DEFAULT_PATH.len() {
        browser.transfer_path_pop();
    }
    "/tmp/lib.toml"
        .chars()
        .for_each(|c| browser.transfer_path_push(c));
    browser.transfer_path_done();
    assert!(!browser.transfer_editing());
    assert_eq!(view(&browser).transfer.path, "/tmp/lib.toml");
}

#[test]
fn export_passes_the_expanded_path_and_scope_and_reports_the_count() {
    let mut browser = on_transfer();
    browser.transfer_toggle_all();
    let seen = Arc::new(Mutex::new(None));
    let out = Arc::clone(&seen);
    browser.transfer_start_export(move |path, all| {
        *out.lock().unwrap() = Some((path, all));
        Ok(3)
    });
    settle(&mut browser);
    let (path, all) = seen.lock().unwrap().clone().unwrap();
    assert!(all);
    assert!(path.is_absolute(), "`~` is expanded: {}", path.display());
    assert!(path.ends_with("gaki-library.toml"));
    let tab = &view(&browser).transfer;
    assert!(
        tab.status.starts_with("Wrote 3 games to "),
        "{}",
        tab.status
    );
    assert!(tab.export.is_none());
}

#[test]
fn a_failed_export_says_why() {
    let mut browser = on_transfer();
    browser.transfer_start_export(|_, _| Err(TuiError::Problem("disk full".into())));
    settle(&mut browser);
    assert_eq!(view(&browser).transfer.status, "Export failed: disk full");
}

#[test]
fn import_lists_installs_then_skips_and_waits_for_an_answer() {
    let mut browser = on_transfer();
    browser.transfer_start_import(|_| {
        Ok(ImportPlan {
            install: vec![lib_entry(1, Some("beta")), lib_entry(2, None)],
            installed: vec![lib_entry(3, None)],
            not_owned: vec![lib_entry(4, None)],
        })
    });
    settle(&mut browser);
    assert!(browser.transfer_confirm_open());
    let tab = &view(&browser).transfer;
    let lines = plan_lines(tab.ready_plan().unwrap());
    assert!(matches!(lines[0], PlanLine::Install(e) if e.app_id == 1));
    assert!(matches!(lines[2], PlanLine::Installed(e) if e.app_id == 3));
    assert!(matches!(lines[3], PlanLine::NotOwned(e) if e.app_id == 4));

    // j/k move within the plan.
    browser.installs_move(10);
    assert_eq!(view(&browser).transfer.selected, 3);

    // `n` drops the plan without touching the backlog.
    let client = Client::new();
    browser.answer_transfer_confirm(&client, false).unwrap();
    assert!(!browser.transfer_confirm_open());
    let tab = &view(&browser).transfer;
    assert!(tab.plan.is_none());
    assert_eq!(tab.status, "Import cancelled.");
}

#[test]
fn an_import_with_nothing_to_install_says_so_without_asking() {
    let mut browser = on_transfer();
    browser.transfer_start_import(|_| {
        Ok(ImportPlan {
            installed: vec![lib_entry(3, None)],
            ..Default::default()
        })
    });
    settle(&mut browser);
    assert!(!browser.transfer_confirm_open());
    assert!(
        view(&browser)
            .transfer
            .status
            .starts_with("Nothing to install")
    );
}

#[test]
fn a_failed_import_read_says_why_and_keeps_no_plan() {
    let mut browser = on_transfer();
    browser.transfer_start_import(|_| Err(TuiError::Problem("no such file".into())));
    settle(&mut browser);
    let tab = &view(&browser).transfer;
    assert!(tab.plan.is_none());
    assert_eq!(tab.status, "Import failed: no such file");
}
