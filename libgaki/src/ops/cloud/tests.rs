use super::*;
use crate::library::cloud_sync::{SyncFailure, SyncSkip};

#[test]
fn a_clean_sync_is_ok() {
    assert_eq!(status_of(&SyncOutcome::default()), "ok");
}

#[test]
fn failures_outrank_skips() {
    let mut outcome = SyncOutcome::default();
    outcome.skipped.push(SyncSkip {
        filename: "a".into(),
        root_token: None,
        reason: "unmapped".into(),
    });
    assert_eq!(status_of(&outcome), "incomplete");
    outcome.failed.push(SyncFailure {
        filename: "b".into(),
        direction: "down".into(),
        error: "timeout".into(),
    });
    assert_eq!(status_of(&outcome), "failed");
}

#[test]
fn directions_have_stable_names() {
    assert_eq!(direction_name(SyncDirection::Both), "both");
    assert_eq!(direction_name(SyncDirection::Up), "up");
}
