use super::super::{ProgressLabel, drive_progress};
use crate::app::game::status::GameStatus;
use libgaki::core::error::{ErrorKind, classify};
use libgaki::core::models::{DownloadProgress, DownloadProgressState as State};
use std::sync::{Arc, Mutex};

fn step(state: State, done: u64, total: u64) -> DownloadProgress {
    DownloadProgress {
        state,
        bytes_downloaded: done,
        total_bytes: total,
        ..Default::default()
    }
}

/// Feed `events` through [`drive_progress`]; returns its result and the
/// status text it left behind.
fn drive(
    label: ProgressLabel,
    events: Vec<DownloadProgress>,
) -> (anyhow::Result<bool>, Option<String>) {
    let status: Arc<Mutex<Option<GameStatus>>> = Arc::new(Mutex::new(None));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let result = runtime.block_on(async {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        for event in events {
            tx.send(event).await.unwrap();
        }
        drop(tx);
        drive_progress(rx, &status, label).await
    });
    let text = status.lock().unwrap().as_ref().map(|s| s.state.clone());
    (result, text)
}

#[test]
fn progress_is_labelled_by_state_and_operation() {
    let (result, text) = drive(
        ProgressLabel::Install,
        vec![
            DownloadProgress::queued("waiting"),
            step(State::Downloading, 25, 100),
        ],
    );
    // The channel closed without a terminal event.
    assert!(!result.unwrap());
    assert_eq!(text.as_deref(), Some("downloading 25.0%"));

    let (_, text) = drive(ProgressLabel::Install, vec![step(State::Moving, 1, 2)]);
    assert_eq!(text.as_deref(), Some("moving 50.0%"));

    let (_, text) = drive(ProgressLabel::Verify, vec![step(State::Downloading, 3, 4)]);
    assert_eq!(text.as_deref(), Some("verifying 75.0%"));

    let (_, text) = drive(
        ProgressLabel::Downgrade,
        vec![step(State::Downloading, 1, 4)],
    );
    assert_eq!(text.as_deref(), Some("downgrading 25.0%"));
    let (_, text) = drive(ProgressLabel::Downgrade, vec![step(State::Verifying, 1, 2)]);
    assert_eq!(text.as_deref(), Some("verifying 50.0%"));
}

#[test]
fn completion_and_failure_end_the_drive() {
    let (result, _) = drive(
        ProgressLabel::Update,
        vec![
            step(State::Downloading, 1, 2),
            DownloadProgress::completed("done"),
            step(State::Downloading, 2, 2),
        ],
    );
    assert!(result.unwrap(), "completion ends the drive");

    let mut failed = step(State::Failed, 0, 0);
    failed.current_file = "disk full".into();
    failed.error_kind = Some(ErrorKind::DiskFull);
    let (result, _) = drive(ProgressLabel::Install, vec![failed]);
    let err = result.unwrap_err();
    assert_eq!(classify(&err).kind, ErrorKind::DiskFull);
    assert!(err.to_string().contains("disk full"));
}
