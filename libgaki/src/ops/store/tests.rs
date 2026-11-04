use super::*;

#[test]
fn defaults_to_us() {
    assert_eq!(resolve_country(None).unwrap(), "US");
}

#[test]
fn normalizes_a_given_code() {
    assert_eq!(resolve_country(Some("uk")).unwrap(), "GB");
}

#[test]
fn rejects_garbage_as_invalid_input() {
    let err = resolve_country(Some("usa")).unwrap_err();
    assert_eq!(
        crate::core::error::classify(&err).kind,
        ErrorKind::InvalidInput
    );
}

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

#[test]
fn assets_list_artwork_then_media() {
    let mut info = StoreAppInfo::default();
    info.assets.header = Some("h".into());
    info.assets.logo = Some("l".into());
    info.screenshots = vec!["s1".into()];
    let kinds: Vec<&str> = assets(&info).iter().map(|a| a.kind).collect();
    assert_eq!(kinds, vec!["header", "logo", "screenshot"]);
}
