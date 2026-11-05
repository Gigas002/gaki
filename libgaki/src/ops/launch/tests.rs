use super::*;

fn game(installed: bool) -> LibraryGame {
    LibraryGame {
        app_id: 1,
        name: "Game".into(),
        playtime_forever_minutes: None,
        is_installed: installed,
        install_path: None,
        local_manifest_ids: Default::default(),
        update_available: false,
        update_queued: false,
        active_branch: "public".into(),
        is_owned: true,
        is_family_shared: false,
        online_required: None,
        platform: None,
    }
}

#[test]
fn an_uninstalled_game_is_refused() {
    assert!(require_installed(&game(true)).is_ok());
    let err = require_installed(&game(false)).unwrap_err();
    assert_eq!(
        crate::core::error::classify(&err).kind,
        ErrorKind::InvalidInput
    );
}

mod build_preference {
    use super::super::BuildPreference;
    use crate::config::Platform;
    use crate::core::config::GameConfig;
    use crate::settings::LaunchSettings;

    fn settings(platform: Option<Platform>) -> LaunchSettings {
        LaunchSettings {
            app_id: 1,
            profile: None,
            platform,
            proton: None,
            runner_args: Vec::new(),
            wrappers: Vec::new(),
            debug: false,
            env: Default::default(),
            args: Vec::new(),
            script: None,
            cloud_sync: true,
            steam: Default::default(),
        }
    }

    fn legacy(platform: Option<&str>, proton: Option<&str>) -> GameConfig {
        GameConfig {
            platform_preference: platform.map(Into::into),
            forced_proton_version: proton.map(Into::into),
            ..GameConfig::default()
        }
    }

    #[test]
    fn resolved_platform_applies_without_a_legacy_setting() {
        let pref = BuildPreference::new(false, &settings(Some(Platform::Linux)), None);
        assert_eq!(pref.platform, Some(Platform::Linux));
        assert_eq!(
            BuildPreference::new(false, &settings(None), None).platform,
            None
        );
    }

    #[test]
    fn legacy_game_setting_beats_the_resolved_platform() {
        let game = legacy(Some("windows"), None);
        let pref = BuildPreference::new(false, &settings(Some(Platform::Linux)), Some(&game));
        assert_eq!(pref.platform, Some(Platform::Windows));

        let game = legacy(None, Some("GE-Proton"));
        let pref = BuildPreference::new(false, &settings(None), Some(&game));
        assert_eq!(
            pref.platform,
            Some(Platform::Windows),
            "a forced Proton means Windows"
        );
        assert_eq!(pref.forced_proton.as_deref(), Some("GE-Proton"));
    }

    #[test]
    fn command_line_beats_the_legacy_setting() {
        let game = legacy(Some("windows"), Some("GE-Proton"));
        let pref = BuildPreference::new(true, &settings(Some(Platform::Linux)), Some(&game));
        assert_eq!(pref.platform, Some(Platform::Linux));
        assert_eq!(pref.forced_proton, None, "no Proton for a Linux launch");
    }
}
