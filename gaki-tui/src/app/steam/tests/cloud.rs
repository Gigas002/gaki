use super::super::sync_problem;
use libgaki::library::cloud_sync::{SyncFailure, SyncOutcome, SyncSkip};

#[test]
fn a_clean_sync_has_no_problem() {
    let outcome = SyncOutcome {
        downloaded: vec!["a.sav".into()],
        uploaded: vec!["b.sav".into()],
        ..Default::default()
    };
    assert_eq!(sync_problem(&outcome), None);
}

#[test]
fn failures_outrank_skips() {
    let outcome = SyncOutcome {
        skipped: vec![SyncSkip {
            filename: "x.sav".into(),
            root_token: None,
            reason: "no prefix".into(),
        }],
        failed: vec![SyncFailure {
            filename: "y.sav".into(),
            direction: "down".into(),
            error: "timed out".into(),
        }],
        ..Default::default()
    };
    let problem = sync_problem(&outcome).unwrap();
    assert!(problem.starts_with("1 file(s) failed"), "{problem}");
    assert!(problem.contains("y.sav: timed out"), "{problem}");

    let skipped_only = SyncOutcome {
        failed: Vec::new(),
        ..outcome
    };
    let problem = sync_problem(&skipped_only).unwrap();
    assert!(
        problem.contains("skipped (first: x.sav: no prefix)"),
        "{problem}"
    );
}

#[test]
fn conflicts_keep_both_copies() {
    let outcome = SyncOutcome {
        conflicts: vec![libgaki::library::cloud_sync::SyncConflict {
            filename: "a.sav".into(),
            local_path: "/tmp/a.sav".into(),
            local_hash: "1".into(),
            local_size: 1,
            local_timestamp: 1,
            cloud_hash: "2".into(),
            cloud_size: 2,
            cloud_timestamp: 2,
        }],
        ..Default::default()
    };
    let problem = sync_problem(&outcome).unwrap();
    assert!(problem.starts_with("1 save(s) differ"), "{problem}");
}
