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
    std::fs::write(dir.path().join("session.json"), "{}").unwrap();
    let issues = leftover_json(dir.path());
    assert_eq!(issues.len(), 4);
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

#[test]
fn env_vars_parse_as_key_value() {
    assert_eq!(
        parse_env_var("DXVK_HUD=fps,gpuload").unwrap(),
        ("DXVK_HUD".to_owned(), "fps,gpuload".to_owned())
    );
    assert_eq!(
        parse_env_var("A=b=c").unwrap(),
        ("A".to_owned(), "b=c".to_owned()),
        "only the first = splits"
    );
    assert_eq!(parse_env_var("EMPTY=").unwrap().1, "");
    assert!(parse_env_var("NOEQUALS").is_err());
    assert!(parse_env_var("=value").is_err());
    assert!(parse_env_var("TWO WORDS=x").is_err());
}

#[test]
fn env_edits_clear_unset_then_set() {
    use std::collections::BTreeMap;
    use toml_edit::{DocumentMut, Item};

    let mut doc: DocumentMut = "env = { A = \"1\", B = \"2\" }\n".parse().unwrap();
    let table = doc.as_table_mut();
    let none = BTreeMap::new();
    assert!(!edit::edit_env(table, false, &[], &none), "a no-op");

    let set = BTreeMap::from([("C".to_owned(), "3".to_owned())]);
    assert!(edit::edit_env(table, false, &["A".to_owned()], &set));
    let env = table.get("env").and_then(Item::as_table).unwrap();
    let keys: Vec<_> = env.iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["B", "C"]);

    assert!(
        !edit::edit_env(table, false, &["GONE".to_owned()], &none),
        "unsetting a missing key changes nothing"
    );

    let replace = EnvChange::replace(BTreeMap::from([("D".to_owned(), "4".to_owned())]));
    assert!(edit::edit_env(
        table,
        replace.clear,
        &replace.unset,
        &replace.set
    ));
    let env = table.get("env").and_then(Item::as_table).unwrap();
    assert_eq!(env.len(), 1);
    assert_eq!(env.get("D").and_then(Item::as_str), Some("4"));

    assert!(edit::edit_env(table, true, &[], &none));
    assert!(table.get("env").is_none(), "an emptied env is removed");
}
