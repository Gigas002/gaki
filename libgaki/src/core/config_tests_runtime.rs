use super::*;

#[test]
fn runtime_settings_default_when_absent() {
    assert_eq!(parse_runtime_settings("{}"), RuntimeSettings::default());
    assert!(parse_runtime_settings("not json").use_keyring);
}

#[test]
fn runtime_settings_ignore_other_fields() {
    let raw =
        r#"{"steam_library_path":"/x","use_keyring":false,"info_cache_ttl":0,"script_dir":"/s"}"#;
    let s = parse_runtime_settings(raw);
    assert!(!s.use_keyring);
    assert_eq!(s.info_cache_ttl, Some(0));
    assert_eq!(s.script_dir.as_deref(), Some("/s"));
}

#[test]
fn new_settings_round_trip_through_launcher_config() {
    let mut config = LauncherConfig {
        use_keyring: false,
        info_cache_ttl: Some(60),
        ..Default::default()
    };
    config.script_dir = Some("/scripts".into());
    let raw = serde_json::to_string(&config).unwrap();
    let back: LauncherConfig = serde_json::from_str(&raw).unwrap();
    assert!(!back.use_keyring);
    assert_eq!(back.info_cache_ttl, Some(60));
    assert_eq!(
        parse_runtime_settings(&raw).script_dir.as_deref(),
        Some("/scripts")
    );
}
