// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use super::*;

#[test]
fn ignores_whitespace_and_padding() {
    assert_eq!(base64_decode("aGVs\nbG8=").unwrap(), b"hello");
    assert_eq!(base64_decode("aGVsbG8").unwrap(), b"hello");
    assert_eq!(base64_decode(" aGVsbG8= \0").unwrap(), b"hello");
}
