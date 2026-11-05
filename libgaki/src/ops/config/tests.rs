use super::*;

#[test]
fn change_map_keeps_the_kind() {
    assert_eq!(Change::<i32>::Keep.map(|v| v + 1), Change::Keep);
    assert_eq!(Change::<i32>::Clear.map(|v| v + 1), Change::Clear);
    assert_eq!(Change::Set(1).map(|v| v + 1), Change::Set(2));
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
fn leftover_json_settings_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    assert!(leftover_json(dir.path()).is_empty());
    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(dir.path().join("user_apps.json"), "{}").unwrap();
    std::fs::write(dir.path().join("collections.json"), "{}").unwrap();
    let issues = leftover_json(dir.path());
    assert_eq!(issues.len(), 3);
    assert!(
        issues[0].message.contains("config.toml"),
        "{}",
        issues[0].message
    );
    assert!(
        issues[1].message.contains("games.toml"),
        "{}",
        issues[1].message
    );
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
