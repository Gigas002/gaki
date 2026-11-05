use crate::core::config::LauncherConfig;
use crate::core::models::LibraryGame;
use crate::infra::runners::{CommandSpec, LaunchContext, Runner, WineTkgRunner};
use crate::steam_client::{LaunchInfo, LaunchTarget};
use std::collections::HashMap;
use std::path::PathBuf;

fn mock_context() -> LaunchContext {
    LaunchContext {
        app: LibraryGame {
            app_id: 123,
            name: "Test Game".to_string(),
            playtime_forever_minutes: None,
            is_installed: true,
            install_path: Some("/tmp/game".to_string()),
            local_manifest_ids: Default::default(),
            update_available: false,
            update_queued: false,
            active_branch: "public".to_string(),
            is_owned: true,
            is_family_shared: false,
            online_required: None,
            platform: None,
        },
        launch_info: LaunchInfo {
            app_id: 123,
            id: "0".to_string(),
            description: "Test".to_string(),
            executable: "game.exe".to_string(),
            arguments: "-v".to_string(),
            workingdir: None,
            target: LaunchTarget::WindowsProton,
        },
        launcher_config: LauncherConfig::default(),
        user_config: None,
        proton_path: Some("/tmp/proton".to_string()),
        settings: None,
        steam_enabled: false,
        target_architecture: crate::core::models::ExecutableArchitecture::X86_64,
        dll_resolutions: Vec::new(),
        game_fixups: Default::default(),
        verification_ptr: std::ptr::null_mut(),
    }
}

#[test]
fn test_launch_context_construction() {
    let ctx = mock_context();
    assert_eq!(ctx.app.app_id, 123);
}

#[test]
fn test_command_spec_construction() {
    let spec = CommandSpec {
        program: PathBuf::from("/usr/bin/wine"),
        args: vec!["game.exe".to_string()],
        env: HashMap::from([("WINEPREFIX".to_string(), "/tmp/prefix".to_string())]),
        ..Default::default()
    };

    assert_eq!(spec.program, PathBuf::from("/usr/bin/wine"));
    assert_eq!(spec.args[0], "game.exe");
    assert_eq!(spec.env.get("WINEPREFIX").unwrap(), "/tmp/prefix");
}

#[tokio::test]
async fn test_wine_tkg_runner_graphics_policy_autodetect() {
    use crate::core::models::{GraphicsBackendPolicy, UserAppConfig};
    use std::fs;
    use tempfile::tempdir;

    let tmp = tempdir().unwrap();
    let lib = tmp.path().join("library");
    let pfx = tmp.path().join("prefix");
    fs::create_dir_all(&lib).unwrap();
    fs::create_dir_all(&pfx).unwrap();

    // Simulate DXVK existence in runner path
    let runner_path = tmp.path().join("proton_8");
    let dxvk_dll = runner_path.join("files/lib/wine/dxvk/d3d11.dll");
    fs::create_dir_all(dxvk_dll.parent().unwrap()).unwrap();
    fs::write(&dxvk_dll, "fake dll content").unwrap();

    let app = LibraryGame {
        app_id: 123,
        name: "Test".to_string(),
        install_path: Some(tmp.path().to_string_lossy().to_string()),
        is_installed: true,
        playtime_forever_minutes: None,
        active_branch: "public".to_string(),
        update_available: false,
        update_queued: false,
        local_manifest_ids: HashMap::new(),
        is_owned: true,
        is_family_shared: false,
        online_required: None,
        platform: None,
    };

    let config = LauncherConfig {
        steam_library_path: lib.to_string_lossy().to_string(),
        ..Default::default()
    };

    let mut user_config = UserAppConfig::default();
    user_config.graphics_layers.graphics_backend_policy = GraphicsBackendPolicy::Auto;

    let ctx = LaunchContext {
        app,
        launch_info: LaunchInfo {
            app_id: 123,
            id: "0".into(),
            description: "Test".into(),
            executable: "test.exe".into(),
            arguments: "".into(),
            workingdir: None,
            target: LaunchTarget::WindowsProton,
        },
        launcher_config: config,
        user_config: Some(user_config.clone()),
        proton_path: Some(runner_path.to_string_lossy().to_string()),
        settings: None,
        steam_enabled: false,
        target_architecture: crate::core::models::ExecutableArchitecture::X86_64,
        dll_resolutions: Vec::new(),
        game_fixups: Default::default(),
        verification_ptr: std::ptr::null_mut(),
    };

    let runner = WineTkgRunner;
    let env = runner.build_env(&ctx).await.unwrap();

    // Auto is now CONSERVATIVE: it should NOT have enabled DXVK even if simulated on disk
    let overrides = env.get("WINEDLLOVERRIDES").unwrap();
    assert!(!overrides.contains("d3d11=n,b"));

    // Explicitly requesting DXVK should still enable it
    let mut user_config_dxvk = user_config.clone();
    user_config_dxvk.graphics_layers.graphics_backend_policy = GraphicsBackendPolicy::DXVK;
    let mut ctx_dxvk = ctx.clone();
    ctx_dxvk.user_config = Some(user_config_dxvk);
    let env_dxvk = runner.build_env(&ctx_dxvk).await.unwrap();
    let overrides_dxvk = env_dxvk.get("WINEDLLOVERRIDES").unwrap();
    // Strict mode uses 'n', not 'n,b'
    assert!(overrides_dxvk.contains("d3d11=n"));
    assert!(!overrides_dxvk.contains("d3d11=n,b"));
}

#[tokio::test]
async fn test_fixup_fragments_merged_into_build_env() {
    // A game with fixups should have both env and DLL-override fragments merged
    // into the built environment.
    let runner = WineTkgRunner;
    let mut ctx = mock_context();
    ctx.game_fixups = crate::launch::fixups::GameFixups {
        env: vec![("PROTON_NO_ESYNC".to_string(), "1".to_string())],
        dll_overrides: vec![("xlive".to_string(), "builtin".to_string())],
    };

    let env = runner.build_env(&ctx).await.unwrap();

    assert_eq!(env.get("PROTON_NO_ESYNC").map(String::as_str), Some("1"));
    let overrides = env.get("WINEDLLOVERRIDES").unwrap();
    assert!(
        overrides.contains("xlive=builtin"),
        "overrides were: {overrides}"
    );
}

#[tokio::test]
async fn test_user_env_wins_over_fixup() {
    use crate::core::models::UserAppConfig;
    let runner = WineTkgRunner;
    let mut ctx = mock_context();
    ctx.game_fixups = crate::launch::fixups::GameFixups {
        env: vec![("PROTON_NO_ESYNC".to_string(), "1".to_string())],
        dll_overrides: vec![],
    };
    let mut user = UserAppConfig::default();
    user.env_variables
        .insert("PROTON_NO_ESYNC".to_string(), "0".to_string());
    ctx.user_config = Some(user);

    let env = runner.build_env(&ctx).await.unwrap();
    // Explicit per-game value must win over the auto-fixup.
    assert_eq!(env.get("PROTON_NO_ESYNC").map(String::as_str), Some("0"));
}

#[tokio::test]
async fn test_wine_tkg_runner_stubs() {
    // We can only test stubs that don't hit the filesystem hard or expect real Proton/Wine
    let runner = WineTkgRunner;
    let ctx = mock_context();

    // build_env should succeed without real filesystem
    let env = runner.build_env(&ctx).await.unwrap();
    assert_eq!(env.get("SteamAppId").unwrap(), "123");
}

#[test]
fn profile_dll_overrides_replace_existing_entries() {
    let overrides = std::collections::BTreeMap::from([
        ("d3d11".to_string(), "b".to_string()),
        ("winhttp".to_string(), "n,b".to_string()),
    ]);
    assert_eq!(
        super::merge_dll_overrides("d3d11,dxgi=n;steamclient=n", &overrides),
        "dxgi=n;steamclient=n;d3d11=b;winhttp=n,b"
    );
    assert_eq!(
        super::merge_dll_overrides("D3D11.dll=n", &overrides),
        "d3d11=b;winhttp=n,b"
    );
    assert_eq!(
        super::merge_dll_overrides("", &overrides),
        "d3d11=b;winhttp=n,b"
    );
}

#[tokio::test]
async fn profile_prefix_and_overrides_reach_the_proton_env() {
    use crate::settings::{LaunchSettings, ProtonBuild, ProtonSettings};

    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("library");
    let eac = lib.join("steamapps/common/Proton EasyAntiCheat Runtime");
    std::fs::create_dir_all(&eac).unwrap();
    let shared = tmp.path().join("prefixes/shared");

    let mut ctx = mock_context();
    ctx.launcher_config.steam_library_path = lib.to_string_lossy().into_owned();
    ctx.settings = Some(LaunchSettings {
        app_id: 123,
        profile: Some("proton".into()),
        platform: None,
        proton: Some(ProtonSettings {
            build: ProtonBuild::Default,
            prefix: Some(shared.clone()),
            dll_overrides: [("winhttp".to_string(), "n,b".to_string())].into(),
            anticheat: crate::config::Anticheat {
                eac: true,
                battleye: false,
            },
        }),
        runner_args: Vec::new(),
        wrappers: Vec::new(),
        debug: true,
        env: Default::default(),
        args: Vec::new(),
        script: None,
        cloud_sync: true,
    });

    let env = WineTkgRunner.build_env(&ctx).await.unwrap();
    assert_eq!(env["STEAM_COMPAT_DATA_PATH"], shared.to_string_lossy());
    assert_eq!(env["WINEPREFIX"], shared.join("pfx").to_string_lossy());
    assert!(
        env["WINEDLLOVERRIDES"].ends_with("winhttp=n,b"),
        "{}",
        env["WINEDLLOVERRIDES"]
    );
    assert_eq!(env["PROTON_EAC_RUNTIME"], eac.to_string_lossy());
    assert!(!env.contains_key("PROTON_BATTLEYE_RUNTIME"));
    assert_eq!(env["PROTON_LOG"], "1");
    assert!(
        !lib.join("steamapps/compatdata/123").exists(),
        "no per-game compatdata with a shared prefix"
    );
}
