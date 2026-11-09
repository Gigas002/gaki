use super::*;

const STEAM_ID: u64 = 76_561_197_960_287_930;

fn jwt(payload: &str) -> String {
    format!(
        "eyJhbGciOiJFZERTQSJ9.{}.c2ln",
        BASE64_URL_SAFE_NO_PAD.encode(payload)
    )
}

#[test]
fn refresh_token_yields_subject_and_expiry() {
    let raw = jwt(&format!(
        r#"{{"iss":"steam","sub":"{STEAM_ID}","exp":4102444800}}"#
    ));
    let token = RefreshToken::new(raw.clone()).unwrap();
    assert_eq!(u64::from(token.subject), STEAM_ID);
    assert_eq!(
        token.expiration,
        SystemTime::UNIX_EPOCH + Duration::from_secs(4_102_444_800)
    );
    assert_eq!(token.token(), raw);
    assert!(!token.expired());
    assert!(!token.is_empty());
}

#[test]
fn past_expiry_is_expired() {
    let token = RefreshToken::new(jwt(&format!(
        r#"{{"iss":"steam","sub":"{STEAM_ID}","exp":1}}"#
    )))
    .unwrap();
    assert!(token.expired());
    assert_eq!(token.remaining_lifetime(), None);
}

#[test]
fn malformed_tokens_are_rejected() {
    assert!(matches!(
        RefreshToken::new("no-dots".into()),
        Err(RefreshTokenError::Malformed(_))
    ));
    assert!(matches!(
        RefreshToken::new("a.!!!.c".into()),
        Err(RefreshTokenError::Malformed(_))
    ));
    assert!(matches!(
        RefreshToken::new(jwt("not json")),
        Err(RefreshTokenError::Malformed(_))
    ));
}

#[test]
fn foreign_issuer_and_bad_subject_are_rejected() {
    assert!(matches!(
        RefreshToken::new(jwt(&format!(r#"{{"iss":"x","sub":"{STEAM_ID}","exp":1}}"#))),
        Err(RefreshTokenError::InvalidIssuer)
    ));
    assert!(matches!(
        RefreshToken::new(jwt(r#"{"iss":"steam","sub":"nope","exp":1}"#)),
        Err(RefreshTokenError::InvalidSubject)
    ));
}

#[test]
fn anonymous_token_is_empty() {
    let id = SteamID::from_steam64(STEAM_ID).unwrap();
    let token = RefreshToken::anonymous(id);
    assert!(token.is_empty());
    assert_eq!(token.token(), "");
}
