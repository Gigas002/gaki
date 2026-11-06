use super::*;

fn record(app_id: u32, name: &str) -> StoreAppInfo {
    StoreAppInfo {
        app_id,
        name: name.to_string(),
        ..Default::default()
    }
}

#[test]
fn dlc_names_are_sorted_and_blank_names_dropped() {
    let records: HashMap<u32, StoreAppInfo> = [record(30, "Thirty"), record(10, "")]
        .into_iter()
        .map(|r| (r.app_id, r))
        .collect();
    let names = names_from_records(&[30, 20, 10], &records);
    assert_eq!(
        names,
        vec![(10, None), (20, None), (30, Some("Thirty".to_string()))]
    );
}

#[test]
fn artwork_prefers_the_tall_library_capsule() {
    let urls = artwork_candidates(440);
    assert!(urls[0].ends_with("/440/library_600x900_2x.jpg"));
    assert!(urls.iter().all(|u| u.contains("/440/")));
}

#[test]
fn single_unknown_app_is_not_found() {
    let err = no_store_info(7);
    assert_eq!(crate::core::error::classify(&err).kind, ErrorKind::NotFound);
}

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
fn assets_list_artwork_then_media() {
    let mut info = StoreAppInfo::default();
    info.assets.header = Some("h".into());
    info.assets.logo = Some("l".into());
    info.screenshots = vec!["s1".into()];
    let kinds: Vec<&str> = assets(&info).iter().map(|a| a.kind).collect();
    assert_eq!(kinds, vec!["header", "logo", "screenshot"]);
}
