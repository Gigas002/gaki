use super::super::sync_problem;
use libgaki::library::cloud_sync::{SyncFailure, SyncOutcome, SyncSkip};

#[test]
fn a_clean_sync_has_no_problem() {
    let outcome = SyncOutcome {
        downloaded: vec!["a.sav".into()],
        uploaded: vec!["b.sav".into()],
        ..Default::default()
    };
    assert_eq!(sync_problem(7, &outcome), None);
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
    let problem = sync_problem(7, &outcome).unwrap();
    assert!(problem.starts_with("1 file(s) failed"), "{problem}");
    assert!(problem.contains("y.sav: timed out"), "{problem}");

    let skipped_only = SyncOutcome {
        failed: Vec::new(),
        ..outcome
    };
    let problem = sync_problem(7, &skipped_only).unwrap();
    assert!(
        problem.contains("skipped (first: x.sav: no prefix)"),
        "{problem}"
    );
}
