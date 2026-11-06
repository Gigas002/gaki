use super::wrap_body;

/// A single word of multibyte (multi-byte-per-char) glyphs, longer than the
/// line width and containing no spaces, forces the hard-split path — the
/// `&word[tail_start..]` byte slice at the heart of `wrap_body`. `tail_start`
/// must land on a char boundary, or this panics. Locks in that invariant.
#[test]
fn wrap_body_hard_splits_multibyte_word_on_char_boundaries() {
    let word = "日".repeat(10); // 10 chars, 30 bytes (3 bytes each)
    let width = 4;
    let lines = wrap_body(&word, width, width);

    // Every produced line fits the width budget measured in CHARS, not bytes.
    for line in &lines {
        assert!(
            line.chars().count() <= width,
            "line {line:?} exceeds {width} columns"
        );
    }
    // The split is lossless: a single word (no spaces) round-trips exactly,
    // which can only hold if every slice fell on a char boundary.
    assert_eq!(lines.concat(), word, "multibyte word must round-trip");
}

/// Mixed ASCII + a too-long emoji run exercises the word-wrap and hard-split
/// paths together; emoji are 4-byte chars, a classic boundary tripwire.
#[test]
fn wrap_body_mixed_ascii_and_emoji_does_not_panic() {
    let body = "hi 🎮🎮🎮🎮🎮 ok";
    let lines = wrap_body(body, 3, 3);

    for line in &lines {
        assert!(line.chars().count() <= 3, "line {line:?} exceeds 3 columns");
    }
    // All non-space glyphs survive the wrap (none dropped or corrupted).
    let original: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    let wrapped: String = lines
        .concat()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(wrapped, original, "no glyph lost or corrupted across wrap");
}
