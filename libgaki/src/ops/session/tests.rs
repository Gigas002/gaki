use super::*;

#[test]
fn unknown_restore_failure_means_log_in_again() {
    let err = restore_failed(ErrorKind::Unknown, None, "boom");
    assert_eq!(classify(&err).kind, ErrorKind::AuthRequired);
    assert!(err.to_string().contains("boom"));
}

#[test]
fn typed_restore_failure_keeps_kind_and_retry() {
    let wait = Some(Duration::from_secs(30));
    let err = restore_failed(ErrorKind::RateLimited, wait, "slow down");
    let c = classify(&err);
    assert_eq!(c.kind, ErrorKind::RateLimited);
    assert_eq!(c.retry_after, wait);
}
