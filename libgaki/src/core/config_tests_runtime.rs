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
}

#[test]
fn new_settings_round_trip_through_launcher_config() {
    let config = LauncherConfig {
        use_keyring: false,
        info_cache_ttl: Some(60),
        ..Default::default()
    };
    let raw = serde_json::to_string(&config).unwrap();
    let back: LauncherConfig = serde_json::from_str(&raw).unwrap();
    assert!(!back.use_keyring);
    assert_eq!(back.info_cache_ttl, Some(60));
    assert_eq!(parse_runtime_settings(&raw).info_cache_ttl, Some(60));
}
