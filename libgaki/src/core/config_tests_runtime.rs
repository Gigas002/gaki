use super::*;

#[test]
fn runtime_settings_default_when_absent() {
    assert_eq!(
        RuntimeSettings::from(&crate::config::Config::default()),
        RuntimeSettings {
            info_cache_ttl: None,
            daemon_idle_timeout: None,
        }
    );
}

#[test]
fn runtime_settings_come_from_config_toml() {
    let config: crate::config::Config =
        toml::from_str("info_cache_ttl = 0\ndaemon_idle_timeout = 60\n").unwrap();
    let settings = RuntimeSettings::from(&config);
    assert_eq!(settings.info_cache_ttl, Some(0));
    assert_eq!(settings.daemon_idle_timeout, Some(60));
}
