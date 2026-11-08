use std::path::{Path, PathBuf};

use super::*;
use crate::config::{Config, Game};

const HOME: &str = "/home/user";

fn proton(name: &str) -> Profile {
    let mut profile = Profile::new(name, Runner::Proton);
    profile.prefix = Some("~/prefixes/shared".into());
    profile.proton = Some(ProtonSection {
        build: Some("experimental".into()),
    });
    profile.wrappers = vec!["gamemoderun".into()];
    profile.env.insert("A".into(), "profile".into());
    profile.env.insert("B".into(), "profile".into());
    profile.dll_overrides.insert("winhttp".into(), "n,b".into());
    profile
}

fn native() -> Profile {
    let mut profile = Profile::new("native", Runner::Native);
    profile.wrappers = vec!["mangohud".into()];
    profile
}

fn catalog(default_profile: Option<&str>, games: Vec<Game>) -> Catalog {
    Catalog {
        config: Config {
            default_profile: default_profile.map(Into::into),
            ..Config::default()
        },
        profiles: vec![native(), proton("proton"), proton("other")],
        games,
        load_errors: Vec::new(),
    }
}

fn run(
    catalog: &Catalog,
    app_id: u32,
    overrides: &Overrides,
) -> Result<LaunchSettings, SettingsError> {
    resolve(catalog, app_id, overrides, Path::new(HOME))
}

#[test]
fn nothing_configured_is_all_defaults() {
    let settings = run(&Catalog::default(), 10, &Overrides::default()).unwrap();
    assert_eq!(settings.profile, None);
    assert_eq!(settings.platform, None);
    assert_eq!(settings.proton, None);
    assert!(settings.env.is_empty() && settings.args.is_empty() && settings.wrappers.is_empty());
    assert!(settings.cloud_sync);
}

#[test]
fn default_profile_applies_without_fixing_the_platform() {
    let settings = run(&catalog(Some("proton"), vec![]), 10, &Overrides::default()).unwrap();
    assert_eq!(settings.profile.as_deref(), Some("proton"));
    assert_eq!(
        settings.platform, None,
        "a default profile doesn't pick the build"
    );
    let proton = settings.proton.unwrap();
    assert_eq!(proton.build, ProtonBuild::Steam("experimental".into()));
    assert_eq!(
        proton.prefix,
        Some(PathBuf::from("/home/user/prefixes/shared"))
    );
    assert_eq!(proton.dll_overrides["winhttp"], "n,b");
    assert_eq!(settings.wrappers, ["gamemoderun"]);
}

#[test]
fn default_profile_is_dropped_when_the_platform_disagrees() {
    let mut game = Game::new(10);
    game.platform = Some(Platform::Linux);
    let settings = run(
        &catalog(Some("proton"), vec![game]),
        10,
        &Overrides::default(),
    )
    .unwrap();
    assert_eq!(settings.profile, None);
    assert_eq!(settings.platform, Some(Platform::Linux));
    assert_eq!(settings.proton, None);
}

#[test]
fn explicit_profile_picks_the_platform() {
    let mut game = Game::new(10);
    game.profile = Some("native".into());
    let settings = run(
        &catalog(Some("proton"), vec![game]),
        10,
        &Overrides::default(),
    )
    .unwrap();
    assert_eq!(settings.profile.as_deref(), Some("native"));
    assert_eq!(settings.platform, Some(Platform::Linux));
    assert_eq!(settings.proton, None);
    assert_eq!(settings.wrappers, ["mangohud"]);

    let overrides = Overrides {
        profile: Some("proton".into()),
        ..Overrides::default()
    };
    let settings = run(&catalog(None, vec![]), 10, &overrides).unwrap();
    assert_eq!(settings.platform, Some(Platform::Windows));
}

#[test]
fn explicit_profile_contradicting_the_platform_is_an_error() {
    let mut game = Game::new(10);
    game.profile = Some("native".into());
    let overrides = Overrides {
        platform: Some(Platform::Windows),
        ..Overrides::default()
    };
    let err = run(&catalog(None, vec![game]), 10, &overrides).unwrap_err();
    assert!(
        matches!(err, SettingsError::PlatformConflict { .. }),
        "{err}"
    );
}

#[test]
fn layers_override_in_order() {
    let mut game = Game::new(10);
    game.profile = Some("other".into());
    game.platform = Some(Platform::Windows);
    game.env.insert("B".into(), "game".into());
    game.args = vec!["-skipIntro".into()];
    game.wrappers = vec!["~/bin/game.sh --fast".into()];
    let mut catalog = catalog(Some("proton"), vec![game]);
    catalog.config.default_platform = Some(Platform::Linux);

    let settings = run(&catalog, 10, &Overrides::default()).unwrap();
    assert_eq!(
        settings.profile.as_deref(),
        Some("other"),
        "game beats default_profile"
    );
    assert_eq!(
        settings.platform,
        Some(Platform::Windows),
        "game beats default_platform"
    );
    assert_eq!(settings.env["A"], "profile");
    assert_eq!(settings.env["B"], "game", "game env beats profile env");
    assert_eq!(settings.args, ["-skipIntro"]);
    assert_eq!(
        settings.wrappers,
        ["gamemoderun", "/home/user/bin/game.sh --fast"],
        "the profile's wrappers, then the game's, `~` expanded"
    );

    let overrides = Overrides {
        profile: Some("proton".into()),
        ..Overrides::default()
    };
    let settings = run(&catalog, 10, &overrides).unwrap();
    assert_eq!(
        settings.profile.as_deref(),
        Some("proton"),
        "CLI beats game"
    );
}

#[test]
fn default_platform_applies_last() {
    let mut catalog = catalog(None, vec![]);
    catalog.config.default_platform = Some(Platform::Windows);
    assert_eq!(
        run(&catalog, 10, &Overrides::default()).unwrap().platform,
        Some(Platform::Windows)
    );
}

#[test]
fn cli_proton_means_windows_and_overrides_the_build() {
    let overrides = Overrides {
        proton: Some("~/compat/proton-ge".into()),
        ..Overrides::default()
    };
    let settings = run(&catalog(Some("proton"), vec![]), 10, &overrides).unwrap();
    assert_eq!(settings.platform, Some(Platform::Windows));
    let proton = settings.proton.unwrap();
    assert_eq!(
        proton.build,
        ProtonBuild::Path("/home/user/compat/proton-ge".into())
    );
    assert!(
        proton.prefix.is_some(),
        "the profile's prefix still applies"
    );

    // Without a proton profile: just the build.
    let settings = run(&Catalog::default(), 10, &overrides).unwrap();
    let proton = settings.proton.unwrap();
    assert_eq!(proton.prefix, None);
    assert!(proton.dll_overrides.is_empty());

    let overrides = Overrides {
        proton: Some("experimental".into()),
        platform: Some(Platform::Linux),
        ..Overrides::default()
    };
    assert_eq!(
        run(&Catalog::default(), 10, &overrides).unwrap_err(),
        SettingsError::ProtonOnLinux(Platform::Linux)
    );
}

#[test]
fn proton_profile_without_a_build_uses_the_default() {
    let mut catalog = catalog(Some("bare"), vec![]);
    catalog.profiles.push(Profile::new("bare", Runner::Proton));
    let proton = run(&catalog, 10, &Overrides::default())
        .unwrap()
        .proton
        .unwrap();
    assert_eq!(proton.build, ProtonBuild::Default);
    assert_eq!(proton.prefix, None);
}

#[test]
fn unknown_and_unsupported_profiles_are_errors() {
    let overrides = Overrides {
        profile: Some("nope".into()),
        ..Overrides::default()
    };
    assert_eq!(
        run(&catalog(None, vec![]), 10, &overrides).unwrap_err(),
        SettingsError::UnknownProfile("nope".into())
    );

    let mut catalog = catalog(Some("wine"), vec![]);
    catalog.profiles.push(Profile::new("wine", Runner::Wine));
    assert!(matches!(
        run(&catalog, 10, &Overrides::default()).unwrap_err(),
        SettingsError::UnsupportedRunner {
            runner: Runner::Wine,
            ..
        }
    ));
}

#[test]
fn other_games_are_unaffected() {
    let mut game = Game::new(10);
    game.args = vec!["-x".into()];
    let settings = run(&catalog(None, vec![game]), 20, &Overrides::default()).unwrap();
    assert!(settings.args.is_empty());
    assert_eq!(settings.app_id, 20);
}

#[test]
fn steam_integration_layers() {
    let mut catalog = catalog(Some("proton"), vec![]);
    assert_eq!(
        run(&catalog, 10, &Overrides::default()).unwrap().steam,
        SteamIntegration::Auto
    );

    catalog.profiles[1].steam = Some(SteamIntegration::None);
    assert_eq!(
        run(&catalog, 10, &Overrides::default()).unwrap().steam,
        SteamIntegration::None,
        "the profile's setting"
    );

    let mut game = Game::new(10);
    game.steam = Some(SteamIntegration::Host);
    catalog.games.push(game);
    assert_eq!(
        run(&catalog, 10, &Overrides::default()).unwrap().steam,
        SteamIntegration::Host,
        "the game beats the profile"
    );

    let overrides = Overrides {
        steam: Some(SteamIntegration::None),
        ..Overrides::default()
    };
    assert_eq!(
        run(&catalog, 10, &overrides).unwrap().steam,
        SteamIntegration::None
    );
}

#[test]
fn shader_cache_layers_game_over_profile_over_config() {
    let mut catalog = catalog(Some("proton"), Vec::new());
    let place = |catalog: &Catalog| {
        run(catalog, 10, &Overrides::default())
            .unwrap()
            .shader_cache
    };
    assert_eq!(place(&catalog), ShaderCachePlace::Default);

    catalog.config.shader_cache = Some(ShaderCache::Dir("~/shaders".into()));
    assert_eq!(
        place(&catalog),
        ShaderCachePlace::Dir(PathBuf::from(HOME).join("shaders"))
    );
    let settings = run(&catalog, 10, &Overrides::default()).unwrap();
    assert_eq!(
        settings.shader_cache_dir(),
        Some(PathBuf::from(HOME).join("shaders/10"))
    );

    catalog.profiles[1].shader_cache = Some(ShaderCache::Off);
    assert_eq!(place(&catalog), ShaderCachePlace::Off);
    assert_eq!(
        run(&catalog, 10, &Overrides::default())
            .unwrap()
            .shader_cache_dir(),
        None
    );

    let mut game = Game::new(10);
    game.shader_cache = Some(ShaderCache::Dir("/fast".into()));
    catalog.games.push(game);
    assert_eq!(place(&catalog), ShaderCachePlace::Dir("/fast".into()));
}
