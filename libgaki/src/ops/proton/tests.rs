use super::*;

#[test]
fn unknown_build_is_not_found_and_explains_custom_builds() {
    let err = find("GE-Proton9-20").unwrap_err();
    assert_eq!(crate::core::error::classify(&err).kind, ErrorKind::NotFound);
    assert!(err.to_string().contains("compatibilitytools.d"));
}

#[test]
fn official_build_resolves() {
    assert_eq!(find("proton experimental").unwrap().app_id, 1493710);
}
