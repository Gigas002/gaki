use super::*;

#[test]
fn change_keep_leaves_the_slot_alone() {
    let mut slot = Some(1);
    assert!(!Change::<i32>::Keep.apply_to(&mut slot));
    assert_eq!(slot, Some(1));
}

#[test]
fn change_clear_and_set_report_a_change() {
    let mut slot = Some(1);
    assert!(Change::Clear.apply_to(&mut slot));
    assert_eq!(slot, None);
    assert!(Change::Set(2).apply_to(&mut slot));
    assert_eq!(slot, Some(2));
}

#[test]
fn empty_proxy_change_is_a_noop() {
    assert!(ProxyChange::default().is_noop());
    let set = ProxyChange {
        url: Some(String::new()),
        ..Default::default()
    };
    assert!(!set.is_noop());
}

#[test]
fn platform_names_match_the_config_spelling() {
    assert_eq!(platform_name(DepotPlatform::Windows), "windows");
    assert_eq!(platform_name(DepotPlatform::Linux), "linux");
}
