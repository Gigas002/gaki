use super::*;

#[test]
fn ok_is_ok() {
    assert!(EResult::from_result(1).is_ok());
}

#[test]
fn known_errors_keep_their_meaning() {
    assert!(matches!(
        EResult::from_result(5),
        Err(EResult::InvalidPassword)
    ));
    assert!(matches!(EResult::from_result(10), Err(EResult::Busy)));
}

#[test]
fn unknown_codes_are_invalid() {
    assert!(matches!(
        EResult::from_result(99_999),
        Err(EResult::Invalid)
    ));
    assert!(matches!(EResult::from_result(4), Err(EResult::Invalid)));
}
