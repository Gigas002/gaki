use super::*;

fn game(installed: bool) -> LibraryGame {
    LibraryGame {
        app_id: 1,
        name: "Game".into(),
        playtime_forever_minutes: None,
        playtime_2weeks_minutes: None,
        last_played: None,
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

mod choose_proton {
    use super::super::choose_proton;
    use crate::config::Platform;
    use crate::settings::{LaunchSettings, ProtonBuild, ProtonSettings};

    fn settings(platform: Option<Platform>, build: Option<ProtonBuild>) -> LaunchSettings {
        LaunchSettings {
            app_id: 1,
            profile: None,
            platform,
            proton: build.map(|build| ProtonSettings {
                build,
                prefix: None,
                dll_overrides: Default::default(),
                anticheat: Default::default(),
            }),
            runner_args: Vec::new(),
            wrappers: Vec::new(),
            debug: false,
            env: Default::default(),
            args: Vec::new(),
            cloud_sync: true,
            steam: Default::default(),
            shader_cache: Default::default(),
        }
    }

    #[test]
    fn the_request_wins() {
        let s = settings(
            Some(Platform::Windows),
            Some(ProtonBuild::Steam("p".into())),
        );
        assert_eq!(choose_proton(Some("GE"), &s, "dflt").as_deref(), Some("GE"));
    }

    #[test]
    fn windows_uses_the_profile_build_then_the_default() {
        let s = settings(
            Some(Platform::Windows),
            Some(ProtonBuild::Steam("p".into())),
        );
        assert_eq!(choose_proton(None, &s, "dflt").as_deref(), Some("p"));
        let s = settings(Some(Platform::Windows), None);
        assert_eq!(choose_proton(None, &s, "dflt").as_deref(), Some("dflt"));
    }

    #[test]
    fn linux_or_no_platform_picks_no_proton() {
        let build = || Some(ProtonBuild::Steam("p".into()));
        assert_eq!(
            choose_proton(None, &settings(Some(Platform::Linux), build()), "d"),
            None
        );
        assert_eq!(choose_proton(None, &settings(None, build()), "d"), None);
    }
}
