use super::*;

#[test]
fn game_config_with_removed_runner_field_still_parses() {
    // Configs written while the umu/luxtorpeda plugins existed carry a `runner` field.
    let legacy = r#"{ "forced_proton_version": "GE-Proton9-20", "runner": "umu" }"#;
    let cfg: GameConfig = serde_json::from_str(legacy).unwrap();
    assert_eq!(cfg.forced_proton_version.as_deref(), Some("GE-Proton9-20"));
}

#[test]
fn game_config_without_pin_fields_defaults_unpinned() {
    // A config written before pinning existed must still parse, unpinned.
    let legacy = r#"{ "forced_proton_version": null, "platform_preference": null }"#;
    let cfg: GameConfig = serde_json::from_str(legacy).unwrap();
    assert!(!cfg.pinned);
    assert!(cfg.pinned_manifests.is_empty());
}

#[test]
fn game_config_pin_round_trips_and_omits_empty() {
    // An unpinned config must not emit the pin fields (skip_serializing_if).
    let unpinned = GameConfig::default();
    let json = serde_json::to_string(&unpinned).unwrap();
    assert!(!json.contains("pinned_manifests"), "got: {json}");

    // A pinned config round-trips its depot→manifest map.
    let mut manifests = HashMap::new();
    manifests.insert(1234u32, 5678u64);
    let pinned = GameConfig {
        pinned: true,
        pinned_manifests: manifests.clone(),
        ..Default::default()
    };
    let json = serde_json::to_string(&pinned).unwrap();
    let back: GameConfig = serde_json::from_str(&json).unwrap();
    assert!(back.pinned);
    assert_eq!(back.pinned_manifests, manifests);
}

#[test]
fn launcher_config_with_removed_plugin_fields_still_parses() {
    // Configs written while the umu/luxtorpeda plugins existed carry their toggles.
    let legacy = r#"{ "steam_library_path": "/x", "proton_version": "experimental",
        "enable_cloud_sync": true, "luxtorpeda_enabled": true, "umu_path": "/opt/umu" }"#;
    let cfg: LauncherConfig = serde_json::from_str(legacy).unwrap();
    assert_eq!(cfg.steam_library_path, "/x");
}

#[test]
fn launcher_config_without_proxy_defaults_to_direct() {
    // A config written before the `proxy` field existed must still parse, direct.
    let legacy = r#"{ "steam_library_path": "/x", "proton_version": "experimental",
        "enable_cloud_sync": true }"#;
    let cfg: LauncherConfig = serde_json::from_str(legacy).unwrap();
    assert_eq!(cfg.proxy, ProxyConfig::default());
    assert!(cfg.proxy.url.is_none());
}

#[test]
fn proxy_config_omits_empty_and_round_trips() {
    // An empty proxy must not emit either field (skip_serializing_if).
    let json = serde_json::to_string(&ProxyConfig::default()).unwrap();
    assert!(!json.contains("url"), "got: {json}");
    assert!(!json.contains("no_proxy"), "got: {json}");

    // A populated proxy round-trips both fields.
    let proxy = ProxyConfig {
        url: Some("socks5://127.0.0.1:1080".to_string()),
        no_proxy: Some("localhost,.internal".to_string()),
    };
    let back: ProxyConfig = serde_json::from_str(&serde_json::to_string(&proxy).unwrap()).unwrap();
    assert_eq!(back, proxy);
}
