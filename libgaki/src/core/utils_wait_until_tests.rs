use super::*;
use std::time::Duration;

#[test]
fn wait_until_returns_once_done() {
    let mut calls = 0;
    let ok = wait_until(
        || {
            calls += 1;
            calls == 3
        },
        Duration::from_secs(5),
        Duration::from_millis(1),
    );
    assert!(ok);
    assert_eq!(calls, 3);
}

#[test]
fn wait_until_gives_up_after_timeout() {
    let start = std::time::Instant::now();
    let ok = wait_until(
        || false,
        Duration::from_millis(20),
        Duration::from_millis(5),
    );
    assert!(!ok);
    assert!(start.elapsed() >= Duration::from_millis(20));
}

#[test]
fn wait_until_checks_at_least_once_with_zero_timeout() {
    assert!(wait_until(
        || true,
        Duration::ZERO,
        Duration::from_millis(1)
    ));
}
