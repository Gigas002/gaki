use super::*;

fn item(app_id: u32) -> QueryItemID {
    let mut id = QueryItemID::new();
    id.set_appid(app_id);
    id
}

#[test]
fn a_page_keeps_apps_and_reads_the_metadata() {
    let mut metadata = service_storequery::CStoreQueryResultMetadata::new();
    metadata.set_total_matching_records(42);
    metadata.spellcheck_suggestions = vec!["portal".to_string()];
    let page = page_from(&[item(10), item(0), item(20)], Some(&metadata));
    assert_eq!(page.app_ids, [10, 20], "non-app items have no app id");
    assert_eq!(page.total, 42);
    assert_eq!(page.suggestions, ["portal"]);
}

#[test]
fn a_page_without_metadata_has_no_total() {
    let page = page_from(&[item(10)], None);
    assert_eq!(page.app_ids, [10]);
    assert_eq!(page.total, 0);
    assert!(page.suggestions.is_empty());
}

#[test]
fn queries_ask_for_games_in_the_users_country() {
    let context = query_context("DE");
    assert_eq!(context.country_code(), "DE");
    assert_eq!(context.language(), crate::core::STEAM_LANGUAGE);
    let types = games_only();
    assert!(types.include_apps() && types.include_games());
    assert!(!types.include_dlc());
}
