use super::*;

#[test]
fn proton_compat_prefix_matches_steam_compat_data_path() {
    // Must stay in step with the STEAM_COMPAT_DATA_PATH the runner exports, or
    // saves land beside the prefix the game actually uses.
    let tmp = tempfile::tempdir().unwrap();
    let mut config = crate::core::config::LauncherConfig::default();
    config.steam_library_path = tmp.path().to_string_lossy().into_owned();
    assert_eq!(
        proton_compat_prefix(&config, 42),
        tmp.path()
            .join("steamapps")
            .join("compatdata")
            .join("42")
            .join("pfx"),
    );
}
