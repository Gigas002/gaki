// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use super::*;

#[test]
fn picks_round_robin_from_start() {
    assert_eq!(pick_index(&[0, 0, 0], 0), Some(0));
    assert_eq!(pick_index(&[0, 0, 0], 4), Some(1));
    assert_eq!(pick_index(&[], 3), None);
}

#[test]
fn skips_benched_hosts() {
    let strikes = [BENCH_STRIKES, 0, BENCH_STRIKES];
    for start in 0..6 {
        assert_eq!(pick_index(&strikes, start), Some(1));
    }
}

#[test]
fn all_benched_takes_least_failing() {
    let strikes = [BENCH_STRIKES + 2, BENCH_STRIKES, BENCH_STRIKES + 1];
    assert_eq!(pick_index(&strikes, 0), Some(1));
}

#[test]
fn backoff_grows_and_caps() {
    assert_eq!(backoff(0), Duration::from_millis(200));
    assert_eq!(backoff(1), Duration::from_millis(400));
    assert_eq!(backoff(3), Duration::from_millis(1600));
    assert_eq!(backoff(9), Duration::from_millis(3000));
}

#[test]
fn only_transfer_errors_retry() {
    assert!(retryable(&Error::Request("reset".into())));
    assert!(retryable(&Error::HttpStatus(
        reqwest::StatusCode::BAD_GATEWAY
    )));
    assert!(retryable(&Error::Decompress("bad zip".into())));
    // Disk errors never retry.
    let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
    assert!(!retryable(&Error::Io(full)));
    assert!(!retryable(&Error::Unexpected("x".into())));
}
