use super::*;

#[test]
fn blank_text_is_none() {
    assert_eq!(nonempty("  Ann  "), Some("Ann".to_string()));
    assert_eq!(nonempty(" \t"), None);
    assert_eq!(nonempty(""), None);
}

#[test]
fn avatar_hashes_become_urls() {
    assert_eq!(
        avatar_url(&[0xab, 0x01, 0xff]).as_deref(),
        Some("https://avatars.cloudflare.steamstatic.com/ab01ff_full.jpg")
    );
    assert_eq!(avatar_url(&[]), None);
    assert_eq!(avatar_url(&[0; 20]), None, "an all-zero hash is no avatar");
}
