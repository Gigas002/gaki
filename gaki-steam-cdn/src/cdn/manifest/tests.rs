// Part of gaki-steam-cdn (gaki's addition to the steam-cdn fork); see README.md.
use std::io::Write;

use base64::Engine;
use gaki_steam_vent_proto::content_manifest::content_manifest_payload::{
    FileMapping, file_mapping::ChunkData as ProtoChunk,
};

use super::*;
use crate::crypto::aes256::encrypt_for_test;

const KEY: [u8; 32] = [5; 32];

fn payload() -> ContentManifestPayload {
    let mut chunk = ProtoChunk::new();
    chunk.set_sha(vec![0xab; 20]);
    chunk.set_offset(0);
    chunk.set_cb_original(4);
    let mut file = FileMapping::new();
    file.set_filename("bin\\game.exe".into());
    file.set_size(4);
    file.set_flags(32);
    file.chunks.push(chunk);
    let mut payload = ContentManifestPayload::new();
    payload.mappings.push(file);
    payload
}

fn metadata() -> ContentManifestMetadata {
    let mut meta = ContentManifestMetadata::new();
    meta.set_depot_id(731);
    meta.set_gid_manifest(42);
    meta.set_filenames_encrypted(true);
    meta.set_cb_disk_original(4);
    meta
}

/// Steam's framing: (magic, length, section) records.
fn framed(sections: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (magic, body) in sections {
        out.extend_from_slice(&magic.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
    }
    out
}

fn zipped(bytes: &[u8]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    let mut w = zip::ZipWriter::new(&mut buf);
    w.start_file::<_, ()>("z", zip::write::SimpleFileOptions::default())
        .unwrap();
    w.write_all(bytes).unwrap();
    w.finish().unwrap();
    buf.into_inner()
}

fn first_name(payload: &ContentManifestPayload) -> &str {
    payload.mappings[0].filename()
}

#[test]
fn reads_framed_sections() {
    let data = framed(&[
        (PROTOBUF_PAYLOAD_MAGIC, payload().write_to_bytes().unwrap()),
        (
            PROTOBUF_METADATA_MAGIC,
            metadata().write_to_bytes().unwrap(),
        ),
        // The signature and end marker are skipped.
        (0x1B81B817, vec![1, 2, 3]),
        (0x32C415AB, Vec::new()),
    ]);
    let (payload, meta) = parse_sections(&data).unwrap();
    assert_eq!(first_name(&payload), "bin\\game.exe");
    assert_eq!(payload.mappings[0].chunks[0].cb_original(), 4);
    let meta = meta.unwrap();
    assert_eq!((meta.depot_id(), meta.gid_manifest()), (731, 42));
    assert!(meta.filenames_encrypted());
}

#[test]
fn reads_a_zipped_manifest() {
    let data = zipped(&framed(&[(
        PROTOBUF_PAYLOAD_MAGIC,
        payload().write_to_bytes().unwrap(),
    )]));
    let (payload, meta) = parse_sections(&data).unwrap();
    assert_eq!(first_name(&payload), "bin\\game.exe");
    assert!(meta.is_none());
}

#[test]
fn reads_a_bare_payload() {
    let body = payload().write_to_bytes().unwrap();
    assert_eq!(
        first_name(&parse_sections(&body).unwrap().0),
        "bin\\game.exe"
    );
    // Behind an 8-byte header that isn't Steam's framing.
    let mut headed = vec![0u8; 8];
    headed.extend_from_slice(&body);
    assert_eq!(
        first_name(&parse_sections(&headed).unwrap().0),
        "bin\\game.exe"
    );
}

#[test]
fn a_section_claiming_too_much_is_not_read_past_the_end() {
    let body = payload().write_to_bytes().unwrap();
    let mut data = framed(&[(PROTOBUF_PAYLOAD_MAGIC, body.clone())]);
    // Claim more bytes than there are.
    data[4..8].copy_from_slice(&(body.len() as u32 + 100).to_le_bytes());
    // The framed read stops; the fallback finds the payload behind the header.
    let (payload, meta) = parse_sections(&data).unwrap();
    assert_eq!(first_name(&payload), "bin\\game.exe");
    assert!(meta.is_none());
}

#[test]
fn garbage_is_an_error() {
    assert!(parse_sections(&[0xff; 32]).is_err());
    // A broken zip falls back to parsing the bytes themselves.
    assert!(parse_sections(b"PK\x03\x04 not a real zip archive").is_err());
}

#[test]
fn decrypts_names() {
    let encrypted = encrypt_for_test(b"bin/game.exe", KEY, [9; 16]);
    let text = base64::engine::general_purpose::STANDARD.encode(encrypted);
    assert_eq!(decrypt_name(&text, KEY).unwrap(), "bin/game.exe");
    assert!(matches!(
        decrypt_name(&text, [6; 32]),
        Err(ManifestError::DecryptFilename(_))
    ));
    // Not UTF-8 after decryption.
    let encrypted = encrypt_for_test(&[0xff, 0xfe], KEY, [9; 16]);
    let text = base64::engine::general_purpose::STANDARD.encode(encrypted);
    assert!(matches!(
        decrypt_name(&text, KEY),
        Err(ManifestError::DecryptFilename(_))
    ));
}
