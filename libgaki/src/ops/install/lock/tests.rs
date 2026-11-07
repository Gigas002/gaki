use super::*;

#[test]
fn a_lock_is_exclusive_until_dropped() {
    let app = 9_100_001;
    let lock = try_acquire(app).unwrap().expect("free at first");
    assert!(try_acquire(app).unwrap().is_none());
    assert!(is_held(app));
    drop(lock);
    assert!(!is_held(app));
    assert!(try_acquire(app).unwrap().is_some());
}

#[test]
fn locks_are_per_app() {
    let _a = try_acquire(9_100_002).unwrap().unwrap();
    assert!(try_acquire(9_100_003).unwrap().is_some());
}
