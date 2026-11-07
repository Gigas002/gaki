use super::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn one_disk_job_at_a_time() {
    let mut b = Browser::new(vec![]);
    let (release, wait) = mpsc::channel::<()>();
    assert!(b.start_disk_job(DiskOp::Move, 1, move || {
        let _ = wait.recv();
        Ok(())
    }));
    assert!(b.disk_job_running());
    // A second one is refused, and says why, while the first runs.
    assert!(!b.start_disk_job(DiskOp::Uninstall, 2, || Ok(())));
    assert!(b.notice.is_some());
    release.send(()).unwrap();
    let slot = &b.disk_job.as_ref().unwrap().slot;
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        if let Some(result) = job::take(slot) {
            break result;
        }
        assert!(Instant::now() < deadline, "the job never finished");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(result, Ok(()));
}

#[test]
fn starting_a_move_marks_its_prompt_busy() {
    let mut b = Browser::new(vec![]);
    b.open_move(7);
    b.move_path = "/nonexistent".into();
    b.disk_job = Some(DiskJob {
        op: DiskOp::Uninstall,
        app_id: 3,
        slot: job::spawn(|| Ok(())),
    });
    // Busy: the prompt keeps its (empty) status and nothing new starts.
    b.do_move();
    assert!(b.move_status.is_empty());
    assert_eq!(b.disk_job.as_ref().unwrap().op, DiskOp::Uninstall);
}

#[test]
fn finished_jobs_report_in_their_prompt_or_the_status_bar() {
    let mut b = Browser::new(vec![]);
    b.open_relink(5);
    b.finish_disk_job(DiskOp::Relink, 5, Err("no such library".into()));
    assert_eq!(b.relink_status, "Failed: no such library");
    assert!(b.notice.is_none());

    // The prompt was closed (or is for another game): the status bar.
    b.close_relink();
    b.finish_disk_job(DiskOp::Relink, 5, Ok(()));
    assert_eq!(b.notice.as_deref(), Some("Relink finished."));
    b.notice = None;
    b.open_import(9);
    b.finish_disk_job(DiskOp::Import, 8, Err("x".into()));
    assert!(b.import_status.is_empty());
    assert_eq!(b.notice.as_deref(), Some("Import failed: x"));

    // Uninstall reports only a failure; success shows in the game's badge.
    b.notice = None;
    b.finish_disk_job(DiskOp::Uninstall, 1, Ok(()));
    assert!(b.notice.is_none());
    b.finish_disk_job(DiskOp::Uninstall, 1, Err("busy".into()));
    assert_eq!(b.notice.as_deref(), Some("Uninstall failed: busy"));
}
