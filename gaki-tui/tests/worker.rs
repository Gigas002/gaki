//! The `libgaki` worker bridge, against an empty config dir (no network).

use std::time::Duration;

use gaki_tui::app::worker::{Request, Response, Worker};

const WAIT: Duration = Duration::from_secs(10);

/// One test: the config dir is set once per process.
#[test]
fn requests_round_trip_without_a_stored_session() {
    let config = tempfile::tempdir().unwrap();
    assert!(libgaki::core::config::set_config_dir(
        config.path().to_path_buf()
    ));
    let worker = Worker::start().unwrap();

    // Before any restore there is no session.
    let status_ticket = worker.request(Request::SessionStatus);
    let reply = worker.wait(WAIT).expect("a status reply");
    assert_eq!(reply.ticket, status_ticket);
    match reply.response {
        Response::SessionStatus(status) => {
            assert!(!status.authenticated);
            assert_eq!(status.account, None);
        }
        other => panic!("unexpected reply: {other:?}"),
    }

    // With nothing stored, a restore succeeds offline and stays logged out.
    let restore_ticket = worker.request(Request::Restore);
    assert_ne!(restore_ticket, status_ticket);
    let reply = worker.wait(WAIT).expect("a restore reply");
    assert_eq!(reply.ticket, restore_ticket);
    match reply.response {
        Response::Restored(Ok(status)) => assert!(!status.authenticated),
        other => panic!("unexpected reply: {other:?}"),
    }

    // Nothing else is pending, and polling doesn't block.
    assert!(worker.poll().is_none());
    drop(worker);
}
