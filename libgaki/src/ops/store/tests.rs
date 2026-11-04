use super::*;

#[test]
fn defaults_to_us() {
    assert_eq!(resolve_country(None).unwrap(), "US");
}

#[test]
fn normalizes_a_given_code() {
    assert_eq!(resolve_country(Some("uk")).unwrap(), "GB");
}

#[test]
fn rejects_garbage_as_invalid_input() {
    let err = resolve_country(Some("usa")).unwrap_err();
    assert_eq!(
        crate::core::error::classify(&err).kind,
        ErrorKind::InvalidInput
    );
}
