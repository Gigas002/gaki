// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use super::*;

/// A VSZ container around `plain`, as Steam builds one.
pub(crate) fn vsz(plain: &[u8]) -> Vec<u8> {
    let frame =
        ruzstd::encoding::compress_to_vec(plain, ruzstd::encoding::CompressionLevel::Fastest);
    let mut out = b"VSZa".to_vec();
    out.extend_from_slice(&crc32fast::hash(plain).to_le_bytes());
    out.extend_from_slice(&frame);
    // crc32, size, then the magic (the exact field layout isn't read).
    out.extend_from_slice(&[0u8; VSZ_FOOTER_LENGTH - 3]);
    out.extend_from_slice(b"zsv");
    out
}

#[test]
fn detects_containers() {
    assert!(is_vz(b"VZa..."));
    assert!(!is_vz(b"V"));
    assert!(is_vsz(b"VSZa"));
    assert!(!is_vsz(b"VSZb"));
    assert!(!is_vsz(b"PK\x03\x04"));
}

#[test]
fn decompresses_vsz() {
    let plain = b"chunk content ".repeat(100);
    assert_eq!(decompress_vsz(&vsz(&plain)).unwrap(), plain);
}

#[test]
fn rejects_bad_vsz() {
    let mut data = vsz(b"abc");
    let n = data.len();
    data[n - 1] = b'x';
    assert!(matches!(decompress_vsz(&data), Err(Error::Decompress(_))));
    assert!(matches!(decompress_vsz(b"VSZa"), Err(Error::Eof(_))));
    assert!(decompress_vsz(b"nope").is_err());
}

#[tokio::test]
async fn vz_needs_its_header() {
    assert!(decompress(b"XXa").await.is_err());
}
