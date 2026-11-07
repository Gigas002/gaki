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
    #[cfg(feature = "collections")]
    collections_round_trip();
    cancelled_qr_login_stops_quietly();
    #[cfg(feature = "versioning")]
    pinning_reads_local_state();
    settings_round_trip();
    game_settings_round_trip();
    proton_default_is_saved();
}

/// What `key` is set to in `config.toml`.
fn setting(key: &str) -> Option<String> {
    steam::settings_values()
        .unwrap()
        .into_iter()
        .find(|(k, _)| *k == key)
        .expect("a known key")
        .1
}

/// Settings edits land in `config.toml` and read back, unquoted; bad values
/// are refused and leave the file alone.
fn settings_round_trip() {
    assert_eq!(setting("presence"), None, "default is unset (offline)");
    steam::settings_set("presence", Some("online")).unwrap();
    assert_eq!(setting("presence").as_deref(), Some("online"));
    steam::settings_set("presence", Some("offline")).unwrap();
    assert_eq!(setting("presence").as_deref(), Some("offline"));
    assert!(steam::settings_set("presence", Some("away")).is_err());

    steam::settings_set("cloud_sync", Some("false")).unwrap();
    assert_eq!(setting("cloud_sync").as_deref(), Some("false"));
    steam::settings_set("info_cache_ttl", Some("60")).unwrap();
    assert_eq!(setting("info_cache_ttl").as_deref(), Some("60"));
    assert!(steam::settings_set("info_cache_ttl", Some("soon")).is_err());
    assert_eq!(setting("info_cache_ttl").as_deref(), Some("60"));
    assert!(
        steam::settings_set("default_profile", Some("ghost")).is_err(),
        "an unknown profile is refused"
    );
    assert!(steam::profiles().is_ok(), "profiles read from an empty dir");
    assert!(
        !steam::config_check().unwrap().iter().any(|f| f.error),
        "a fresh config has no errors"
    );
    steam::settings_set("cloud_sync", None).unwrap();
    steam::settings_set("info_cache_ttl", None).unwrap();
    assert_eq!(setting("cloud_sync"), None);
    assert_eq!(setting("info_cache_ttl"), None);

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

/// Per-game settings set in `games.toml`, read back, and clear.
fn game_settings_round_trip() {
    let id = 945360;
    let cfg = steam::config_game_show(id).unwrap();
    assert_eq!((cfg.profile, cfg.platform, cfg.steam), (None, None, None));

    steam::config_game_set_platform(id, Some("windows")).unwrap();
    steam::config_game_set_steam(id, Some("host")).unwrap();
    let cfg = steam::config_game_show(id).unwrap();
    assert_eq!(cfg.platform.as_deref(), Some("windows"));
    assert_eq!(cfg.steam.as_deref(), Some("host"));
    assert_eq!(cfg.effective_steam, "host");

    assert!(steam::config_game_set_platform(id, Some("macos")).is_err());
    assert!(steam::config_game_set_steam(id, Some("maybe")).is_err());
    assert!(
        steam::config_game_set_profile(id, Some("ghost")).is_err(),
        "an unknown profile is rejected"
    );
    assert_eq!(steam::config_game_show(id).unwrap().profile, None);

    steam::config_game_set_platform(id, None).unwrap();
    steam::config_game_set_steam(id, None).unwrap();
    let cfg = steam::config_game_show(id).unwrap();
    assert_eq!((cfg.profile, cfg.platform, cfg.steam), (None, None, None));
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
        setting("default_proton").as_deref(),
        Some(marked[0].as_str())
    );
}

/// Steam-backed game data asks for a login instead of failing obscurely.
fn game_data_needs_a_session() {
    let id = 945360;
    let calls: Vec<(&str, Result<(), gaki_tui::utils::error::TuiError>)> = vec![
        ("achievements", steam::achievements(id).map(drop)),
        ("dlc", steam::dlc(id).map(drop)),
        ("depots", steam::depots(id).map(drop)),
        ("launch options", steam::launch_options(id).map(drop)),
        #[cfg(feature = "versioning")]
        ("branches", steam::branches(id).map(drop)),
        #[cfg(feature = "versioning")]
        ("set branch", steam::set_branch(id, "beta")),
        #[cfg(feature = "versioning")]
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
#[cfg(feature = "versioning")]
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
/// (wallet, inventory, market listings) fail without one too. Only the
/// calls this build has are checked.
fn social_needs_a_session() {
    type Outcome = Result<(), gaki_tui::utils::error::TuiError>;
    #[allow(unused_mut)]
    let mut needs_login: Vec<(&str, Outcome)> = vec![
        ("game info", steam::game_info(945360).map(drop)),
        ("drm check", steam::drm_check(945360).map(drop)),
    ];
    #[cfg(feature = "social")]
    needs_login.extend([
        ("friends", steam::friends().map(drop)),
        ("friends search", steam::friends_search("gaben").map(drop)),
        ("friends add", steam::friends_add("gaben")),
        ("friends remove", steam::friends_remove(76561197960287930)),
        (
            "chat history",
            steam::chat_history(76561197960287930, 5).map(drop),
        ),
        ("chat send", steam::chat_send(76561197960287930, "hi")),
    ]);
    #[cfg(feature = "workshop")]
    needs_login.extend([
        ("workshop status", steam::workshop_status(945360).map(drop)),
        (
            "workshop browse",
            steam::workshop_browse(945360, "").map(drop),
        ),
        ("workshop subscribe", steam::workshop_subscribe(1)),
        ("workshop rate", steam::workshop_rate(1, true)),
        (
            "workshop install",
            steam::workshop_install(vec![1], Default::default()).map(drop),
        ),
        (
            "workshop uninstall",
            steam::workshop_uninstall(vec![1]).map(drop),
        ),
        (
            "workshop comment",
            steam::workshop_post_comment(1, "hi".to_string()).map(drop),
        ),
        (
            "workshop comments",
            steam::workshop_comments(1, 5).map(drop),
        ),
    ]);
    for (what, result) in needs_login {
        let err = result.expect_err(what);
        assert!(
            err.to_string().contains("not logged in"),
            "{what}: unexpected error: {err}"
        );
    }
    #[allow(unused_mut)]
    let mut web: Vec<(&str, Outcome)> = Vec::new();
    #[cfg(feature = "inventory")]
    web.extend([
        ("wallet", steam::wallet().map(drop)),
        ("inventory", steam::inventory(440).map(drop)),
    ]);
    #[cfg(feature = "market")]
    web.push(("market listings", steam::market_listings().map(drop)));
    for (what, result) in web {
        assert!(result.is_err(), "{what} needs a session");
    }
}

/// Collections are edited in the local store offline; only the Steam sync
/// needs a login.
#[cfg(feature = "collections")]
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
