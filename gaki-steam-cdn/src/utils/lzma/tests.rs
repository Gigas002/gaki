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

/// A VZ container around `plain`, as Steam builds one: raw LZMA with the
/// properties byte and dictionary size up front, no end marker.
pub(crate) fn vz(plain: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let options = lzma_rust2::LzmaOptions::with_preset(6);
    let mut writer =
        lzma_rust2::LzmaWriter::new(Vec::new(), &options, false, false, Some(plain.len() as u64))
            .unwrap();
    writer.write_all(plain).unwrap();
    let stream = writer.finish().unwrap();
    let mut out = b"VZa".to_vec();
    out.extend_from_slice(&[0u8; 4]); // timestamp/crc field, not read
    out.push(options.get_props());
    out.extend_from_slice(&options.dict_size.to_le_bytes());
    out.extend_from_slice(&stream);
    out.extend_from_slice(&crc32fast::hash(plain).to_le_bytes());
    out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
    out.extend_from_slice(b"zv");
    out
}

#[tokio::test]
async fn decompresses_vz() {
    let plain = b"chunk content ".repeat(100);
    assert_eq!(decompress(&vz(&plain), plain.len()).await.unwrap(), plain);
}

#[tokio::test]
async fn vz_checks_crc_size_and_stream() {
    let plain = b"chunk content ".repeat(100);
    let good = vz(&plain);
    let n = good.len();

    let mut bad_crc = good.clone();
    bad_crc[n - 10] ^= 1;
    assert!(matches!(
        decompress(&bad_crc, plain.len()).await,
        Err(Error::Decompress(_))
    ));

    // Claims more than the chunk may hold: refused before decoding.
    assert!(matches!(
        decompress(&good, plain.len() - 1).await,
        Err(Error::Decompress(_))
    ));

    // Claims more than the stream holds.
    let mut long = good.clone();
    long[n - 6..n - 2].copy_from_slice(&(plain.len() as u32 + 1).to_le_bytes());
    assert!(matches!(
        decompress(&long, plain.len() + 1).await,
        Err(Error::Decompress(_))
    ));

    // Garbage in the LZMA stream.
    let mut garbled = good;
    for b in &mut garbled[12..n - 10] {
        *b = 0xff;
    }
    assert!(matches!(
        decompress(&garbled, plain.len()).await,
        Err(Error::Decompress(_))
    ));
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
    assert_eq!(decompress_vsz(&vsz(&plain), plain.len()).unwrap(), plain);
}

#[test]
fn rejects_bad_vsz() {
    let mut data = vsz(b"abc");
    let n = data.len();
    data[n - 1] = b'x';
    assert!(matches!(
        decompress_vsz(&data, 3),
        Err(Error::Decompress(_))
    ));
    assert!(matches!(decompress_vsz(b"VSZa", 3), Err(Error::Eof(_))));
    assert!(decompress_vsz(b"nope", 3).is_err());
}

#[tokio::test]
async fn vz_needs_its_header() {
    assert!(decompress(b"XXa", 64).await.is_err());
    // Too short to hold a header and footer: an error, not an underflow.
    assert!(matches!(decompress(b"VZa", 64).await, Err(Error::Eof(_))));
}

#[test]
fn bounded_read_refuses_more() {
    assert_eq!(read_bounded(&b"abcd"[..], 4).unwrap(), b"abcd");
    assert!(matches!(
        read_bounded(&b"abcde"[..], 4),
        Err(Error::Decompress(_))
    ));
}
