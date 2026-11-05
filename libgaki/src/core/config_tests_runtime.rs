use super::*;

#[test]
fn runtime_settings_default_when_absent() {
    assert_eq!(
        RuntimeSettings::from(&crate::config::Config::default()),
        RuntimeSettings {
            info_cache_ttl: None
        }
    );
}

#[test]
fn runtime_settings_come_from_config_toml() {
    let config: crate::config::Config = toml::from_str("info_cache_ttl = 0\n").unwrap();
    assert_eq!(RuntimeSettings::from(&config).info_cache_ttl, Some(0));
}
