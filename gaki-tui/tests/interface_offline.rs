//! Interface calls that now run in-process on `libgaki`, against an empty
//! config dir: nothing stored, so nothing reaches the network.

use gaki_tui::interface::aurelia;

/// One test: the config dir is set once per process.
#[test]
fn logged_out_session_is_reported_not_fetched() {
    let config = tempfile::tempdir().unwrap();
    assert!(libgaki::core::config::set_config_dir(
        config.path().to_path_buf()
    ));

    let health = aurelia::health().expect("health works offline");
    assert!(!health.logged_in);
    assert_eq!(health.account, None);
    assert!(!health.daemon);

    let err = aurelia::account().expect_err("no session, no account");
    assert!(
        err.to_string().contains("not logged in"),
        "unexpected error: {err}"
    );

    // Nothing was launched from this config dir.
    assert!(aurelia::running().unwrap().is_empty());
    assert!(aurelia::stop(945360).is_err(), "nothing to stop");

    let err = aurelia::launch_targets(945360).expect_err("targets need a session");
    assert!(
        err.to_string().contains("not logged in"),
        "unexpected error: {err}"
    );

    // Logging out is idempotent: with nothing stored it still succeeds.
    aurelia::logout().expect("logout without a session");
    assert!(!aurelia::health().unwrap().logged_in);

    settings_round_trip();
}

/// Presence and proxy edits land in the launcher config and read back.
fn settings_round_trip() {
    assert!(
        !aurelia::config_show().unwrap().is_online(),
        "default is offline"
    );
    aurelia::set_presence(true).unwrap();
    assert!(aurelia::config_show().unwrap().is_online());
    aurelia::set_presence(false).unwrap();
    assert!(!aurelia::config_show().unwrap().is_online());

    assert_eq!(aurelia::config_proxy_show().unwrap().url, None);
    aurelia::config_proxy_set("http://127.0.0.1:8080").unwrap();
    aurelia::config_proxy_set_no_proxy("localhost, .lan").unwrap();
    let proxy = aurelia::config_proxy_show().unwrap();
    assert_eq!(proxy.url.as_deref(), Some("http://127.0.0.1:8080"));
    assert_eq!(proxy.no_proxy.as_deref(), Some("localhost, .lan"));

    assert!(
        aurelia::config_proxy_set("not a url").is_err(),
        "invalid URLs are rejected"
    );
    assert_eq!(
        aurelia::config_proxy_show().unwrap().url.as_deref(),
        Some("http://127.0.0.1:8080"),
        "a rejected URL leaves the setting alone"
    );

    aurelia::config_proxy_clear().unwrap();
    let proxy = aurelia::config_proxy_show().unwrap();
    assert_eq!((proxy.url, proxy.no_proxy), (None, None));
}
