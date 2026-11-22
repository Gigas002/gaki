use super::*;

#[test]
fn regions_are_split_normalized_and_deduplicated() {
    let lists = vec!["us, de".to_string(), "UK".to_string(), "US".to_string()];
    assert_eq!(parse_regions(&lists).unwrap(), vec!["US", "DE", "GB"]);
}

#[test]
fn a_bad_region_is_invalid_input() {
    let err = parse_regions(&["US,xyz".to_string()]).unwrap_err();
    assert_eq!(
        crate::core::error::classify(&err).kind,
        ErrorKind::InvalidInput
    );
}

#[test]
fn records_follow_request_order() {
    let mut records: Vec<StoreAppInfo> = [3, 1, 2]
        .into_iter()
        .map(|app_id| StoreAppInfo {
            app_id,
            ..Default::default()
        })
        .collect();
    sort_by_request(&mut records, &[2, 3, 1]);
    let ids: Vec<u32> = records.iter().map(|r| r.app_id).collect();
    assert_eq!(ids, vec![2, 3, 1]);
}

#[test]
fn quotes_mark_region_locked_apps_unavailable() {
    let info = StoreAppInfo {
        country: "DE".into(),
        price: Some("9,99€".into()),
        region_locked: true,
        ..Default::default()
    };
    assert!(!PriceQuote::from_info(&info).available);
    let free = StoreAppInfo {
        is_free: true,
        ..Default::default()
    };
    assert!(PriceQuote::from_info(&free).available);
}

fn draft() -> ReviewDraft {
    ReviewDraft {
        text: "  Good game.\n".into(),
        voted_up: true,
        is_public: false,
        language: None,
        received_compensation: true,
        comments_disabled: false,
    }
}

#[test]
fn a_new_review_posts_the_store_form() {
    let form = recommend_form(440, &draft());
    let get = |key: &str| form.iter().find(|(k, _)| *k == key).unwrap().1.as_str();
    assert_eq!(get("appid"), "440");
    assert_eq!(get("steamworksappid"), "440");
    assert_eq!(get("comment"), "Good game.");
    assert_eq!(get("rated_up"), "true");
    assert_eq!(get("is_public"), "false");
    assert_eq!(get("language"), "english");
    assert_eq!(get("received_compensation"), "true");
    assert_eq!(get("disable_comments"), "false");
    // `sessionid` is the web session's to add.
    assert!(form.iter().all(|(k, _)| *k != "sessionid"));
}

#[test]
fn the_form_answer_is_read() {
    assert!(recommend_result(r#"{"success":true}"#).is_ok());
    assert!(recommend_result(r#"{"success":1}"#).is_ok());
    let err = recommend_result(r#"{"success":false,"strError":"Too short"}"#).unwrap_err();
    assert_eq!(err.to_string(), "Steam refused the review: Too short");
    let err = recommend_result(r#"{"success":2}"#).unwrap_err();
    assert!(err.to_string().contains("own the game"), "{err}");
    assert!(recommend_result("<html>").is_err());
}

#[test]
fn a_draft_needs_text_within_the_limit() {
    let mut d = draft();
    assert_eq!(d.check(), None);
    d.text = " \n".into();
    assert!(d.check().unwrap().contains("needs some text"));
    d.text = "x".repeat(ReviewDraft::MAX_CHARS + 1);
    assert!(d.check().unwrap().contains("at most 8000"));
}
