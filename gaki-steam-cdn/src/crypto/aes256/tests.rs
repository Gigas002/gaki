// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use super::*;

const KEY: [u8; 32] = [7; 32];
const IV: [u8; IV_LENGTH] = [3; IV_LENGTH];

#[test]
fn round_trips_steam_format() {
    for plain in [&b""[..], b"x", b"exactly sixteen!", b"depot/file name.dll"] {
        let mut data = encrypt_for_test(plain, KEY, IV);
        assert_eq!(
            decrypt_cbc_with_iv_extraction(&mut data, KEY).unwrap(),
            plain
        );
    }
}

#[test]
fn wrong_key_fails_unpadding() {
    let mut data = encrypt_for_test(b"secret", KEY, IV);
    assert!(decrypt_cbc_with_iv_extraction(&mut data, [8; 32]).is_err());
}

#[test]
fn short_input_is_an_error() {
    assert!(decrypt_cbc_with_iv_extraction(&mut [0u8; 4], KEY).is_err());
    assert!(decrypt_cbc_with_iv_extraction(&mut [0u8; IV_LENGTH + 3], KEY).is_err());
}
