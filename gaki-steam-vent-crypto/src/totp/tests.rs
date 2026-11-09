use super::*;

// Vectors from another-steam-totp, which this replaces.
const BASE64_SECRET: &str = "000000000000000000000000000=";
const HEX_SECRET: &str = "D34D34D34D34D34D34D34D34D34D34D34D34D34D";
const AT: u64 = 1_634_603_498;

#[test]
fn base64_secret_gives_known_code() {
    let secret = SharedSecret::decode(BASE64_SECRET).unwrap();
    assert_eq!(secret.auth_code_at(AT), "2C5H2");
}

#[test]
fn hex_secret_is_the_same_key() {
    let secret = SharedSecret::decode(HEX_SECRET).unwrap();
    assert_eq!(secret.auth_code_at(AT), "2C5H2");
}

#[test]
fn code_holds_for_its_30_second_window() {
    let secret = SharedSecret::decode(BASE64_SECRET).unwrap();
    let window_start = AT - AT % 30;
    assert_eq!(secret.auth_code_at(window_start), "2C5H2");
    assert_eq!(secret.auth_code_at(window_start + 29), "2C5H2");
    assert_ne!(secret.auth_code_at(window_start + 30), "2C5H2");
}

#[test]
fn codes_use_steams_alphabet() {
    let secret = SharedSecret::decode(BASE64_SECRET).unwrap();
    for t in (0..3000).step_by(30) {
        let code = secret.auth_code_at(t);
        assert_eq!(code.len(), 5);
        assert!(code.bytes().all(|c| ALPHABET.contains(&c)), "{code}");
    }
}

#[test]
fn garbage_and_empty_secrets_are_rejected() {
    assert!(SharedSecret::decode("").is_err());
    assert!(SharedSecret::decode("not base64 !").is_err());
}

#[test]
fn debug_hides_the_secret() {
    let secret = SharedSecret::decode(HEX_SECRET).unwrap();
    assert_eq!(format!("{secret:?}"), "SharedSecret(..)");
}
