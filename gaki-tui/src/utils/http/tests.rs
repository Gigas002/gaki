#[test]
fn a_failed_request_is_a_miss() {
    // Nothing listens on the discard port; the connection is refused.
    assert_eq!(super::get("http://127.0.0.1:9/"), None);
    assert_eq!(super::get("not a url"), None);
}
