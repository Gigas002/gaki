use super::*;

#[test]
fn guard_code_names_match_the_json_protocol() {
    assert_eq!(GuardCode::Email.as_str(), "email");
    assert_eq!(GuardCode::Device.as_str(), "device");
}

#[test]
fn garbage_session_file_counts_as_none() {
    assert_eq!(classify_stored_session("not json"), StoredSession::None);
}

#[test]
fn empty_session_counts_as_none() {
    assert_eq!(classify_stored_session("{}"), StoredSession::None);
}

#[test]
fn persona_name_wins_over_account_name() {
    let raw = r#"{"account_name":"acct","persona_name":"Persona","refresh_token":"t"}"#;
    assert_eq!(
        classify_stored_session(raw),
        StoredSession::Account("Persona".into())
    );
}

#[test]
fn blank_names_fall_back_to_account_then_unknown() {
    let raw = r#"{"account_name":"acct","persona_name":"  "}"#;
    assert_eq!(
        classify_stored_session(raw),
        StoredSession::Account("acct".into())
    );
    let raw = r#"{"web_token":"t"}"#;
    assert_eq!(classify_stored_session(raw), StoredSession::Unknown);
}
