use super::*;

#[test]
fn launcher_config_fills_defaults() {
    let config = LauncherConfig::from_config(&crate::config::Config::default(), None);
    assert_eq!(config.proton_version, DEFAULT_PROTON);
    assert!(config.use_keyring && config.enable_cloud_sync);
    assert!(!config.encrypt_session);
    assert_eq!(config.proxy, ProxyConfig::default());
}

#[test]
fn launcher_config_takes_the_toml_settings() {
    let config: crate::config::Config = toml::from_str(
        "steam_library = \"~/games/steam\"\n\
         default_proton = \"GE-Proton9\"\n\
         keyring = false\n\
         proxy = \"socks5://127.0.0.1:1080\"\n",
    )
    .unwrap();
    let config = LauncherConfig::from_config(&config, Some(Path::new("/home/u")));
    assert_eq!(config.steam_library_path, "/home/u/games/steam");
    assert_eq!(config.proton_version, "GE-Proton9");
    assert!(!config.use_keyring);
    assert_eq!(config.proxy.url.as_deref(), Some("socks5://127.0.0.1:1080"));
}

#[test]
fn info_cache_keeps_dlc_without_a_name() {
    let record = CachedAppInfo {
        fetched_at: 7,
        details: crate::steam_client::StoreAppInfo {
            app_id: 570,
            name: "Dota 2".into(),
            price_cents: Some(-1),
            ..Default::default()
        },
        dlc: vec![(1, Some("Pack".into())), (2, None)],
    };
    let text = toml::to_string(&record).unwrap();
    let back: CachedAppInfo = toml::from_str(&text).unwrap();
    assert_eq!(back.dlc, record.dlc);
    assert_eq!(back.details.name, "Dota 2");
}

#[test]
fn library_cache_keeps_large_manifest_ids() {
    let game = OwnedGame {
        app_id: 570,
        name: "Dota 2".into(),
        playtime_forever_minutes: 3,
        local_manifest_ids: HashMap::from([(573, u64::MAX)]),
        update_available: true,
    };
    let text = toml::to_string(&LibraryCache { games: vec![game] }).unwrap();
    let back: LibraryCache = toml::from_str(&text).unwrap();
    assert_eq!(back.games[0].local_manifest_ids[&573], u64::MAX);
}

#[test]
fn pins_round_trip_with_large_manifest_ids() {
    let mut pins = Pins::new();
    pins.insert(570, HashMap::from([(573, u64::MAX), (571, 7)]));
    pins.insert(10, HashMap::new());
    let text = render_pins(&pins);
    assert!(text.contains(&format!("573 = \"{}\"", u64::MAX)), "{text}");
    assert!(text.find("[10]") < text.find("[570]"), "sorted: {text}");
    assert_eq!(parse_pins(&text).unwrap(), pins);
    assert!(parse_pins("").unwrap().is_empty());
}

#[test]
fn bad_pins_are_errors() {
    assert!(
        parse_pins("[570]\n571 = 7\n").is_err(),
        "manifest ids are strings"
    );
    assert!(parse_pins("[abc]\n").is_err());
    assert!(parse_pins("[570]\n571 = \"x\"\n").is_err());
}

#[test]
fn xdg_dir_uses_an_absolute_value() {
    let dir = xdg_dir(Some("/xdg/state".into()), || unreachable!(), ".local/state").unwrap();
    assert_eq!(dir, PathBuf::from("/xdg/state/gaki"));
}

#[test]
fn xdg_dir_ignores_an_unset_or_relative_value() {
    let home = || Ok(PathBuf::from("/home/u"));
    let expected = PathBuf::from("/home/u/.cache/gaki");
    assert_eq!(xdg_dir(None, home, ".cache").unwrap(), expected);
    assert_eq!(
        xdg_dir(Some("rel/cache".into()), home, ".cache").unwrap(),
        expected
    );
}

#[test]
fn xdg_dir_ignores_an_empty_value() {
    // An empty `XDG_CONFIG_HOME=` is relative, so it falls back too.
    let home = || Ok(PathBuf::from("/home/u"));
    let dir = xdg_dir(Some("".into()), home, ".config").unwrap();
    assert_eq!(dir, PathBuf::from("/home/u/.config/gaki"));
}
