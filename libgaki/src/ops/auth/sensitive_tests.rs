use super::*;

#[test]
fn without_reauth_a_grant_is_free() {
    let fresh = Mutex::new(None);
    assert!(grant(false, &fresh, Instant::now()).is_ok());
    assert!(grant(false, &fresh, Instant::now()).is_ok());
}

#[test]
fn reauth_needs_a_login_and_uses_it_up() {
    let now = Instant::now();
    let fresh = Mutex::new(None);
    let err = grant(true, &fresh, now).unwrap_err();
    assert_eq!(
        crate::core::error::classify(&err).kind,
        ErrorKind::AuthRequired
    );

    *fresh.lock().unwrap() = Some(now);
    assert!(grant(true, &fresh, now).is_ok());
    // One login, one action.
    assert!(grant(true, &fresh, now).is_err());
}

#[test]
fn an_old_login_is_not_fresh() {
    let now = Instant::now();
    let fresh = Mutex::new(now.checked_sub(REAUTH_WINDOW + Duration::from_secs(1)));
    assert!(grant(true, &fresh, now).is_err());
    assert!(fresh_login_at_hand(Some(now), now + REAUTH_WINDOW));
    assert!(!fresh_login_at_hand(None, now));
}
