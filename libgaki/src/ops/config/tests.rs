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

#[test]
fn leftover_launch_scripts_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    assert!(leftover_scripts(dir.path()).is_empty());
    let scripts = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join("570.sh"), "exec \"$@\"").unwrap();
    std::fs::write(scripts.join("notes.txt"), "").unwrap();
    let issues = leftover_scripts(dir.path());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, crate::config::Severity::Warning);
    assert!(
        issues[0].message.contains("config game 570 --wrapper"),
        "{}",
        issues[0].message
    );
}
