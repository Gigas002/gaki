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

    game_data_needs_a_session();
    pinning_reads_local_state();
    settings_round_trip();
    game_settings_round_trip();
    scripts_round_trip();
    proton_default_is_saved();
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

/// Per-game overrides set, read back, and clear.
fn game_settings_round_trip() {
    let id = 945360;
    let cfg = aurelia::config_game_show(id).unwrap();
    assert_eq!(cfg.forced_proton_version, None);
    assert_eq!(cfg.platform_preference, None);

    aurelia::config_game_set_proton(id, "Proton 9.0").unwrap();
    aurelia::config_game_set_platform(id, "windows").unwrap();
    let cfg = aurelia::config_game_show(id).unwrap();
    assert_eq!(cfg.forced_proton_version.as_deref(), Some("Proton 9.0"));
    assert_eq!(cfg.platform_preference.as_deref(), Some("windows"));

    assert!(aurelia::config_game_set_platform(id, "macos").is_err());

    aurelia::config_game_clear_proton(id).unwrap();
    aurelia::config_game_clear_platform(id).unwrap();
    let cfg = aurelia::config_game_show(id).unwrap();
    assert_eq!(cfg.forced_proton_version, None);
    assert_eq!(cfg.platform_preference, None);
}

/// A scaffolded launch script shows up, refuses a second scaffold, and goes
/// away again.
fn scripts_round_trip() {
    let id = 945360;
    assert!(!aurelia::scripts_show(id).unwrap().exists);

    let path = aurelia::scripts_new(id, false).unwrap();
    let shown = aurelia::scripts_show(id).unwrap();
    assert!(shown.exists);
    assert_eq!(shown.path.as_deref(), Some(path.as_str()));
    assert!(shown.contents.unwrap_or_default().contains("945360"));
    assert!(
        aurelia::scripts_new(id, false).is_err(),
        "no silent overwrite"
    );

    aurelia::scripts_remove(id).unwrap();
    assert!(!aurelia::scripts_show(id).unwrap().exists);
    aurelia::scripts_remove(id).expect("removing nothing is fine");
}

/// The official builds are always listed, and a new default is marked.
fn proton_default_is_saved() {
    let protons = aurelia::proton_list().unwrap();
    let official = protons
        .iter()
        .find(|p| p.official)
        .expect("official builds are listed")
        .name
        .clone();
    aurelia::proton_default(&official).unwrap();
    let marked: Vec<_> = aurelia::proton_list()
        .unwrap()
        .into_iter()
        .filter(|p| p.is_default)
        .map(|p| p.name)
        .collect();
    assert_eq!(marked, [official]);
    assert_eq!(
        aurelia::config_show().unwrap().proton_version.as_deref(),
        Some(marked[0].as_str())
    );
}

/// Steam-backed game data asks for a login instead of failing obscurely.
fn game_data_needs_a_session() {
    let id = 945360;
    let calls: [(&str, Result<(), gaki_tui::util::error::STError>); 7] = [
        ("achievements", aurelia::achievements(id).map(drop)),
        ("dlc", aurelia::dlc(id).map(drop)),
        ("depots", aurelia::depots(id).map(drop)),
        ("launch options", aurelia::launch_options(id).map(drop)),
        ("branches", aurelia::branches(id).map(drop)),
        ("set branch", aurelia::set_branch(id, "beta")),
        ("manifests", aurelia::manifests(id, None).map(drop)),
    ];
    for (what, result) in calls {
        let err = result.expect_err(what);
        assert!(
            err.to_string().contains("not logged in"),
            "{what}: unexpected error: {err}"
        );
    }
}

/// Pin state comes from local files, so it works logged out.
fn pinning_reads_local_state() {
    let id = 945360;
    let state = aurelia::available(id).unwrap();
    assert!(!state.available && !state.pinned);
    assert!(state.pinned_manifests.is_empty());

    let err = aurelia::pin(id).expect_err("nothing installed to pin");
    assert!(err.to_string().contains("no installed depots"), "{err}");
    assert!(!aurelia::unpin(id).unwrap(), "was not pinned");
}
