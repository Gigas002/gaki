use super::*;
use secrecy::ExposeSecret;

/// The unit-test process shares one config dir and one memory store, so the
/// session tests run as one sequence.
#[tokio::test]
async fn session_round_trip() {
    assert!(load_session().await.unwrap().refresh_token.is_none());
    assert_eq!(stored_account(), StoredAccount::None);

    // A session.json from before the keyring asks for a login.
    let legacy = legacy_session_path().unwrap();
    std::fs::write(&legacy, r#"{"refresh_token":"old"}"#).unwrap();
    let err = format!("{:#}", load_session().await.unwrap_err());
    assert!(err.contains("gaki-cli login"), "{err}");

    let session = SessionState {
        account_name: Some("acct".into()),
        persona_name: Some("Persona".into()),
        steam_id: Some(76561198000000001),
        refresh_token: Some("refresh-secret".into()),
        web_token: Some("web-secret".into()),
        client_instance_id: None,
    };
    save_session(&session).await.unwrap();
    assert!(!legacy.exists(), "a save removes the old session.json");

    let text = std::fs::read_to_string(account_path().unwrap()).unwrap();
    assert!(!text.contains("secret"), "no token in account.toml: {text}");
    assert!(text.contains("refresh_token = true"), "{text}");

    let back = load_session().await.unwrap();
    let expose = |t: &Option<SecretString>| t.as_ref().map(|t| t.expose_secret().to_owned());
    assert_eq!(
        expose(&back.refresh_token).as_deref(),
        Some("refresh-secret")
    );
    assert_eq!(expose(&back.web_token).as_deref(), Some("web-secret"));
    assert_eq!(stored_account(), StoredAccount::Account("Persona".into()));

    // A token the store lost is an error, not a silent logout.
    session_store()
        .delete(&web_token_key(76561198000000001))
        .unwrap();
    let err = format!("{:#}", load_session().await.unwrap_err());
    assert!(err.contains("web-token") && err.contains("login"), "{err}");

    // Switching accounts drops the old account's tokens.
    let other = SessionState {
        account_name: Some("other".into()),
        steam_id: Some(76561198000000002),
        refresh_token: Some("r2".into()),
        ..SessionState::default()
    };
    save_session(&other).await.unwrap();
    assert_eq!(
        session_store()
            .get(&refresh_token_key(76561198000000001))
            .unwrap(),
        None
    );

    delete_session().await.unwrap();
    assert!(!account_path().unwrap().exists());
    assert_eq!(
        session_store()
            .get(&refresh_token_key(76561198000000002))
            .unwrap(),
        None
    );
    assert_eq!(load_session().await.unwrap().steam_id, None);
}

#[test]
fn a_token_needs_a_steam_id() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = SessionState {
        refresh_token: Some("t".into()),
        ..SessionState::default()
    };
    assert!(rt.block_on(save_session(&session)).is_err());
}

#[test]
fn stored_account_prefers_the_persona_name() {
    let account = |persona: Option<&str>, name: Option<&str>, token: bool| AccountFile {
        persona_name: persona.map(Into::into),
        account_name: name.map(Into::into),
        refresh_token: token,
        ..AccountFile::default()
    };
    assert_eq!(classify(None), StoredAccount::None);
    assert_eq!(
        classify(Some(account(Some("P"), Some("a"), true))),
        StoredAccount::Account("P".into())
    );
    assert_eq!(
        classify(Some(account(Some(" "), Some("a"), true))),
        StoredAccount::Account("a".into())
    );
    assert_eq!(
        classify(Some(account(None, None, true))),
        StoredAccount::Unknown
    );
    assert_eq!(
        classify(Some(account(None, None, false))),
        StoredAccount::None
    );
}
