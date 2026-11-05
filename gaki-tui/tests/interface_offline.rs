//! Interface calls that now run in-process on `libgaki`, against an empty
//! config dir: nothing stored, so nothing reaches the network.

use gaki_tui::app::steam;

/// One test: the config dir is set once per process.
#[test]
fn logged_out_session_is_reported_not_fetched() {
    let config = tempfile::tempdir().unwrap();
    assert!(libgaki::core::config::set_config_dir(
        config.path().to_path_buf()
    ));

    let health = steam::health().expect("health works offline");
    assert!(!health.logged_in);
    assert_eq!(health.account, None);
    assert!(!health.daemon);

    let err = steam::account().expect_err("no session, no account");
    assert!(
        err.to_string().contains("not logged in"),
        "unexpected error: {err}"
    );

    // Nothing was launched from this config dir.
    assert!(steam::running().unwrap().is_empty());
    assert!(steam::stop(945360).is_err(), "nothing to stop");

    let err = steam::launch_targets(945360).expect_err("targets need a session");
    assert!(
        err.to_string().contains("not logged in"),
        "unexpected error: {err}"
    );

    // Logging out is idempotent: with nothing stored it still succeeds.
    steam::logout().expect("logout without a session");
    assert!(!steam::health().unwrap().logged_in);

    game_data_needs_a_session();
    social_needs_a_session();
    collections_round_trip();
    cancelled_qr_login_stops_quietly();
    pinning_reads_local_state();
    settings_round_trip();
    game_settings_round_trip();
    scripts_round_trip();
    proton_default_is_saved();
}

/// Presence and proxy edits land in the launcher config and read back.
fn settings_round_trip() {
    assert!(
        !steam::config_show().unwrap().is_online(),
        "default is offline"
    );
    steam::set_presence(true).unwrap();
    assert!(steam::config_show().unwrap().is_online());
    steam::set_presence(false).unwrap();
    assert!(!steam::config_show().unwrap().is_online());

    assert_eq!(steam::config_proxy_show().unwrap().url, None);
    steam::config_proxy_set("http://127.0.0.1:8080").unwrap();
    steam::config_proxy_set_no_proxy("localhost, .lan").unwrap();
    let proxy = steam::config_proxy_show().unwrap();
    assert_eq!(proxy.url.as_deref(), Some("http://127.0.0.1:8080"));
    assert_eq!(proxy.no_proxy.as_deref(), Some("localhost, .lan"));

    assert!(
        steam::config_proxy_set("not a url").is_err(),
        "invalid URLs are rejected"
    );
    assert_eq!(
        steam::config_proxy_show().unwrap().url.as_deref(),
        Some("http://127.0.0.1:8080"),
        "a rejected URL leaves the setting alone"
    );

    steam::config_proxy_clear().unwrap();
    let proxy = steam::config_proxy_show().unwrap();
    assert_eq!((proxy.url, proxy.no_proxy), (None, None));
}

/// Per-game overrides set, read back, and clear.
fn game_settings_round_trip() {
    let id = 945360;
    let cfg = steam::config_game_show(id).unwrap();
    assert_eq!(cfg.forced_proton_version, None);
    assert_eq!(cfg.platform_preference, None);

    steam::config_game_set_proton(id, "Proton 9.0").unwrap();
    steam::config_game_set_platform(id, "windows").unwrap();
    let cfg = steam::config_game_show(id).unwrap();
    assert_eq!(cfg.forced_proton_version.as_deref(), Some("Proton 9.0"));
    assert_eq!(cfg.platform_preference.as_deref(), Some("windows"));

    assert!(steam::config_game_set_platform(id, "macos").is_err());

    steam::config_game_clear_proton(id).unwrap();
    steam::config_game_clear_platform(id).unwrap();
    let cfg = steam::config_game_show(id).unwrap();
    assert_eq!(cfg.forced_proton_version, None);
    assert_eq!(cfg.platform_preference, None);
}

/// A scaffolded launch script shows up, refuses a second scaffold, and goes
/// away again.
fn scripts_round_trip() {
    let id = 945360;
    assert!(!steam::scripts_show(id).unwrap().exists);

    let path = steam::scripts_new(id, false).unwrap();
    let shown = steam::scripts_show(id).unwrap();
    assert!(shown.exists);
    assert_eq!(shown.path.as_deref(), Some(path.as_str()));
    assert!(shown.contents.unwrap_or_default().contains("945360"));
    assert!(
        steam::scripts_new(id, false).is_err(),
        "no silent overwrite"
    );

    steam::scripts_remove(id).unwrap();
    assert!(!steam::scripts_show(id).unwrap().exists);
    steam::scripts_remove(id).expect("removing nothing is fine");
}

/// The official builds are always listed, and a new default is marked.
fn proton_default_is_saved() {
    let protons = steam::proton_list().unwrap();
    let official = protons
        .iter()
        .find(|p| p.official)
        .expect("official builds are listed")
        .name
        .clone();
    steam::proton_default(&official).unwrap();
    let marked: Vec<_> = steam::proton_list()
        .unwrap()
        .into_iter()
        .filter(|p| p.is_default)
        .map(|p| p.name)
        .collect();
    assert_eq!(marked, [official]);
    assert_eq!(
        steam::config_show().unwrap().proton_version.as_deref(),
        Some(marked[0].as_str())
    );
}

/// Steam-backed game data asks for a login instead of failing obscurely.
fn game_data_needs_a_session() {
    let id = 945360;
    let calls: [(&str, Result<(), gaki_tui::utils::error::TuiError>); 7] = [
        ("achievements", steam::achievements(id).map(drop)),
        ("dlc", steam::dlc(id).map(drop)),
        ("depots", steam::depots(id).map(drop)),
        ("launch options", steam::launch_options(id).map(drop)),
        ("branches", steam::branches(id).map(drop)),
        ("set branch", steam::set_branch(id, "beta")),
        ("manifests", steam::manifests(id, None).map(drop)),
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
    let state = steam::available(id).unwrap();
    assert!(!state.available && !state.pinned);
    assert!(state.pinned_manifests.is_empty());

    let err = steam::pin(id).expect_err("nothing installed to pin");
    assert!(err.to_string().contains("no installed depots"), "{err}");
    assert!(!steam::unpin(id).unwrap(), "was not pinned");
}

/// Friends, chat and Workshop ask for a login; the web-surface calls
/// (wallet, inventory, market listings) fail without one too.
fn social_needs_a_session() {
    type Outcome = Result<(), gaki_tui::utils::error::TuiError>;
    let needs_login: [(&str, Outcome); 11] = [
        ("friends", steam::friends().map(drop)),
        ("friends search", steam::friends_search("gaben").map(drop)),
        ("friends add", steam::friends_add("gaben")),
        ("friends remove", steam::friends_remove(76561197960287930)),
        (
            "chat history",
            steam::chat_history(76561197960287930, 5).map(drop),
        ),
        ("chat send", steam::chat_send(76561197960287930, "hi")),
        ("workshop list", steam::workshop_list(945360).map(drop)),
        (
            "workshop browse",
            steam::workshop_browse(945360, "").map(drop),
        ),
        ("workshop subscribe", steam::workshop_subscribe(1)),
        ("workshop rate", steam::workshop_rate(1, true)),
        (
            "workshop comments",
            steam::workshop_comments(1, 5).map(drop),
        ),
    ];
    for (what, result) in needs_login {
        let err = result.expect_err(what);
        assert!(
            err.to_string().contains("not logged in"),
            "{what}: unexpected error: {err}"
        );
    }
    let web: [(&str, Outcome); 3] = [
        ("wallet", steam::wallet().map(drop)),
        ("inventory", steam::inventory(440).map(drop)),
        ("market listings", steam::market_listings().map(drop)),
    ];
    for (what, result) in web {
        assert!(result.is_err(), "{what} needs a session");
    }
}

/// Collections are edited in the local store offline; only the Steam sync
/// needs a login.
fn collections_round_trip() {
    steam::collection_create("Shooters").unwrap();
    steam::collection_add("Shooters", &[440, 730]).unwrap();
    let shown = steam::collection_show("Shooters").unwrap();
    assert_eq!(shown.kind, "static");
    assert!(!shown.dynamic);
    let mut members = shown.app_ids.clone();
    members.sort_unstable();
    assert_eq!(members, [440, 730]);

    steam::collection_remove("Shooters", &[730]).unwrap();
    steam::collection_rename("Shooters", "Arena").unwrap();
    let listed = steam::collections_list().unwrap();
    let arena = listed
        .iter()
        .find(|c| c.name == "Arena")
        .expect("renamed collection is listed");
    assert_eq!(arena.count, Some(1));
    assert!(listed.iter().all(|c| c.name != "Shooters"));

    assert!(steam::collection_add("Arena", &[-1]).is_err(), "bad app id");

    steam::collection_delete("Arena").unwrap();
    assert!(
        steam::collections_list()
            .unwrap()
            .iter()
            .all(|c| c.name != "Arena"),
        "deleted collections are hidden"
    );

    for (what, result) in [
        ("pull", steam::collections_pull()),
        ("push", steam::collection_push()),
        ("sync", steam::collection_sync()),
    ] {
        let err = result.expect_err(what);
        assert!(err.to_string().contains("not logged in"), "{what}: {err}");
    }
}

/// A cancelled QR attempt ends at once and never publishes a code.
fn cancelled_qr_login_stops_quietly() {
    use std::sync::{Arc, Mutex};
    let cell = Arc::new(Mutex::new(None));
    let started = std::time::Instant::now();
    let err = steam::login_qr(&cell, || true).expect_err("cancelled");
    assert!(err.to_string().contains("cancelled"), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(*cell.lock().unwrap(), None);
}
