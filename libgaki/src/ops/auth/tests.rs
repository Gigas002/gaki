use super::*;

#[test]
fn guard_code_names_match_the_json_protocol() {
    assert_eq!(GuardCode::Email.as_str(), "email");
    assert_eq!(GuardCode::Device.as_str(), "device");
}
