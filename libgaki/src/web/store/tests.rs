use super::*;

fn envelope(json: &str) -> HashMap<String, Envelope> {
    serde_json::from_str(json).unwrap()
}

#[test]
fn tags_go_and_entities_decode() {
    assert_eq!(
        strip_html("  <b>Tom &amp; Jerry</b> &lt;3 &quot;hi&quot; it&#39;s&nbsp;fine\r\n "),
        "Tom & Jerry <3 \"hi\" it's fine"
    );
    assert_eq!(strip_html("<p></p>"), "");
}

#[test]
fn requirement_blobs_become_lines() {
    let html = "<strong>Minimum:</strong><br><ul class=\"bb_ul\"><li><strong>OS:</strong> Windows 10</li>\
                <li><strong>Memory:</strong> 8 GB RAM</li></ul><br/>Notes &amp; more<br />";
    assert_eq!(
        requirements_lines(html),
        ["OS: Windows 10", "Memory: 8 GB RAM", "Notes & more"],
        "the Minimum: heading is dropped"
    );
    assert!(requirements_lines("<strong>Recommended:</strong>").is_empty());
}

#[test]
fn windows_requirements_come_first() {
    let map = envelope(
        r#"{"10": {"success": true, "data": {
            "pc_requirements": {"minimum": "<li>Windows</li>", "recommended": "<li>More</li>"},
            "linux_requirements": {"minimum": "<li>Linux</li>"}
        }}}"#,
    );
    let req = requirements_from(&map, 10).unwrap();
    assert_eq!(req.minimum, ["Windows"]);
    assert_eq!(req.recommended, ["More"]);
}

#[test]
fn an_empty_platform_falls_through_to_the_next() {
    // Steam sends `[]` for a platform with no requirements.
    let map = envelope(
        r#"{"10": {"success": true, "data": {
            "pc_requirements": [],
            "linux_requirements": {"minimum": "<li>Linux</li>"}
        }}}"#,
    );
    let req = requirements_from(&map, 10).unwrap();
    assert_eq!(req.minimum, ["Linux"]);
    assert!(req.recommended.is_empty());

    let map = envelope(r#"{"10": {"success": true, "data": {"pc_requirements": []}}}"#);
    let req = requirements_from(&map, 10).unwrap();
    assert!(req.minimum.is_empty() && req.recommended.is_empty());
}

#[test]
fn a_failed_or_missing_app_has_no_requirements() {
    let map = envelope(r#"{"10": {"success": false}}"#);
    assert!(requirements_from(&map, 10).is_none());
    assert!(requirements_from(&map, 20).is_none());
}

#[test]
fn bbcode_tags_go_and_blocks_break() {
    assert_eq!(
        strip_bbcode(
            "[h2]About[/h2][p]A [b]bold[/b] [url=https://x.y]link[/url].[/p]\
             [img src=\"{STEAM_APP_IMAGE}/a.avif\" fromclient=1][/img][list][*]one[*]two[/list]"
        ),
        "\nAbout\n\nA bold link.\n\n\n• one\n• two\n"
    );
}

#[test]
fn bracketed_text_is_not_a_tag() {
    assert_eq!(
        strip_bbcode("Agent [REDACTED] cites [1] and [x y"),
        "Agent [REDACTED] cites [1] and [x y"
    );
}
