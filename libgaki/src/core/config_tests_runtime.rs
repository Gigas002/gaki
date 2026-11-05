use super::*;

#[test]
fn runtime_settings_default_when_absent() {
    assert_eq!(
        RuntimeSettings::from(&crate::config::Config::default()),
        RuntimeSettings {
            use_keyring: true,
            info_cache_ttl: None
        }
    );
}

#[test]
fn runtime_settings_come_from_config_toml() {
    let config: crate::config::Config =
        toml::from_str("keyring = false\ninfo_cache_ttl = 0\n").unwrap();
    let settings = RuntimeSettings::from(&config);
    assert!(!settings.use_keyring);
    assert_eq!(settings.info_cache_ttl, Some(0));
}
