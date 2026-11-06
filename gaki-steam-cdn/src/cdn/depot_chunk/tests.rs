// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use std::io::Write;

use super::*;
use crate::crypto::aes256::encrypt_for_test;

const KEY: [u8; 32] = [9; 32];

fn zipped(plain: &[u8]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = zip::ZipWriter::new(&mut buf);
    w.start_file::<_, ()>("z", zip::write::SimpleFileOptions::default())
        .unwrap();
    w.write_all(plain).unwrap();
    w.finish().unwrap();
    buf.into_inner()
}

#[tokio::test]
async fn reads_a_zipped_chunk() {
    let mut data = encrypt_for_test(&zipped(b"zip chunk"), KEY, [1; 16]);
    assert_eq!(
        decrypt_and_decompress(&mut data, KEY).await.unwrap(),
        b"zip chunk"
    );
}

#[tokio::test]
async fn reads_a_vsz_chunk() {
    let plain = b"zstd chunk".repeat(50);
    let mut data = encrypt_for_test(&crate::utils::lzma::tests::vsz(&plain), KEY, [2; 16]);
    assert_eq!(decrypt_and_decompress(&mut data, KEY).await.unwrap(), plain);
}

#[tokio::test]
async fn rejects_short_data() {
    assert!(matches!(
        decrypt_and_decompress(&mut [0u8; 8], KEY).await,
        Err(Error::Eof(_))
    ));
}
