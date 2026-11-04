use super::*;

fn item(id: u64, title: &str, hcontent_file: u64) -> WorkshopItem {
    WorkshopItem {
        id,
        app_id: 1,
        creator: 0,
        title: title.into(),
        hcontent_file,
        file_url: String::new(),
        file_size: 0,
        time_updated: 0,
        kind: WorkshopItemKind::Item,
        children: Vec::new(),
    }
}

#[test]
fn ids_union_keeps_installed_first_without_duplicates() {
    assert_eq!(union_ids(&[(1, 10), (2, 20)], &[2, 3]), vec![1, 2, 3]);
}

#[test]
fn a_changed_manifest_is_an_update() {
    let installed = [(1, 10), (2, 20)];
    let details = [item(1, "Same", 10), item(2, "Newer", 21)];
    let rows = status_rows(&[1, 2, 3], &installed, &[2, 3], &details);
    assert!(!rows[0].update_available);
    assert!(rows[1].update_available && rows[1].subscribed);
    assert_eq!(
        rows[2],
        ItemStatus {
            id: 3,
            title: String::new(),
            installed: false,
            subscribed: true,
            update_available: false,
        }
    );
}

#[test]
fn a_zero_manifest_is_not_an_update() {
    let rows = status_rows(&[1], &[(1, 10)], &[], &[item(1, "Legacy", 0)]);
    assert!(!rows[0].update_available);
}

#[test]
fn sort_maps_to_steam_query_types() {
    assert_eq!(Sort::Trend.query_type(), 3);
    assert_eq!(Sort::Text.query_type(), 12);
}
