use super::*;

const MIN: Duration = Duration::from_secs(60);

#[test]
fn the_daemon_idles_out_only_when_quiet_for_the_timeout() {
    assert!(!idle_expired(false, MIN, Some(2 * MIN)));
    assert!(idle_expired(false, 2 * MIN, Some(2 * MIN)));
    // Running work keeps it alive however long it has been.
    assert!(!idle_expired(true, 10 * MIN, Some(2 * MIN)));
    // No timeout (`daemon_idle_timeout = 0`) never expires.
    assert!(!idle_expired(false, 1000 * MIN, None));
}
