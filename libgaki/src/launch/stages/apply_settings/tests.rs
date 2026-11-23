use std::collections::BTreeMap;

use super::*;

fn settings() -> LaunchSettings {
    LaunchSettings {
        sruman: None,
        app_id: 10,
        profile: None,
        platform: None,
        proton: None,
        runner_args: Vec::new(),
        wrappers: Vec::new(),
        debug: false,
        env: BTreeMap::new(),
        args: Vec::new(),
        cloud_sync: true,
        steam: Default::default(),
        shader_cache: Default::default(),
    }
}

fn spec() -> CommandSpec {
    let mut spec = CommandSpec {
        program: "/proton/proton".into(),
        args: vec!["run".into(), "/game/game.exe".into()],
        ..CommandSpec::default()
    };
    spec.env.insert("A".into(), "runner".into());
    spec
}

#[test]
fn nothing_configured_leaves_the_command_alone() {
    let mut command = spec();
    apply(&settings(), &mut command);
    assert_eq!(command.program, std::path::Path::new("/proton/proton"));
    assert_eq!(command.args, ["run", "/game/game.exe"]);
}

#[test]
fn env_and_args_are_added() {
    let mut s = settings();
    s.env.insert("A".into(), "profile".into());
    s.env.insert("B".into(), "1".into());
    s.args = vec!["-skipIntro".into()];
    let mut command = spec();
    apply(&s, &mut command);
    assert_eq!(
        command.env["A"], "profile",
        "configured env wins over the runner's"
    );
    assert_eq!(command.env["B"], "1");
    assert_eq!(command.args, ["run", "/game/game.exe", "-skipIntro"]);
}

#[test]
fn wrappers_nest_outermost_first() {
    let mut s = settings();
    s.wrappers = vec!["mangohud --dlsym".into(), "gamemoderun".into()];
    let mut command = spec();
    apply(&s, &mut command);
    assert_eq!(command.program, std::path::Path::new("mangohud"));
    assert_eq!(
        command.args,
        [
            "--dlsym",
            "gamemoderun",
            "/proton/proton",
            "run",
            "/game/game.exe"
        ]
    );
}

#[test]
fn gaki_variables_describe_the_unwrapped_command() {
    let mut command = spec();
    crate::launch::game_env::export(
        &mut command,
        10,
        "Game",
        Some(std::path::Path::new("/games/g")),
    );
    let mut s = settings();
    s.wrappers = vec!["/home/u/run.sh".into()];
    apply(&s, &mut command);
    assert_eq!(command.program, std::path::Path::new("/home/u/run.sh"));
    assert_eq!(command.env["GAKI_APP_ID"], "10");
    assert_eq!(command.env["GAKI_APP_NAME"], "Game");
    assert_eq!(command.env["GAKI_GAME_DIR"], "/games/g");
    assert_eq!(command.env["GAKI_LAUNCH_PROGRAM"], "/proton/proton");
    assert_eq!(command.env["GAKI_LAUNCH_ARGS"], "run /game/game.exe");
}

#[test]
fn shader_cache_vars_yield_to_runner_inherited_and_user_env() {
    let mut command = spec();
    command
        .env
        .insert("STEAM_COMPAT_DATA_PATH".into(), "/compat".into());
    command
        .env
        .insert("VKD3D_SHADER_CACHE_PATH".into(), "/fixup".into());
    let dir = std::path::Path::new("/cache/10");
    shader_cache_env(&mut command, dir, |key| key == "MESA_SHADER_CACHE_DIR");
    let mut settings = settings();
    settings
        .env
        .insert("DXVK_STATE_CACHE_PATH".into(), "/mine".into());
    apply(&settings, &mut command);

    let env = &command.env;
    assert_eq!(env["STEAM_COMPAT_SHADER_PATH"], "/cache/10");
    assert_eq!(env["__GL_SHADER_DISK_CACHE_PATH"], "/cache/10/nvidiav1");
    assert_eq!(env["VKD3D_SHADER_CACHE_PATH"], "/fixup");
    assert_eq!(env["DXVK_STATE_CACHE_PATH"], "/mine");
    assert!(!env.contains_key("MESA_SHADER_CACHE_DIR"));
}

#[test]
fn native_launches_get_no_proton_variable() {
    let mut command = spec();
    shader_cache_env(&mut command, std::path::Path::new("/c/10"), |_| false);
    assert_eq!(command.env["MESA_SHADER_CACHE_DIR"], "/c/10");
    assert!(!command.env.contains_key("STEAM_COMPAT_SHADER_PATH"));
}

#[test]
fn proton_gets_the_transcoded_media_only_when_it_was_fetched() {
    let dir = tempfile::tempdir().unwrap();
    let proton = || {
        let mut command = spec();
        command
            .env
            .insert("STEAM_COMPAT_DATA_PATH".into(), "/compat".into());
        command
    };
    let mut command = proton();
    shader_cache_env(&mut command, dir.path(), |_| false);
    assert!(
        !command
            .env
            .contains_key("STEAM_COMPAT_TRANSCODED_MEDIA_PATH")
    );

    std::fs::write(dir.path().join("transcoded_video.foz"), b"x").unwrap();
    let mut command = proton();
    shader_cache_env(&mut command, dir.path(), |_| false);
    assert_eq!(
        command.env["STEAM_COMPAT_TRANSCODED_MEDIA_PATH"],
        dir.path().to_string_lossy()
    );

    // A native launch has no use for it.
    let mut command = spec();
    shader_cache_env(&mut command, dir.path(), |_| false);
    assert!(
        !command
            .env
            .contains_key("STEAM_COMPAT_TRANSCODED_MEDIA_PATH")
    );
}

#[test]
fn an_slr_profile_hands_the_game_to_sruman_inside_the_wrappers() {
    let mut s = settings();
    s.sruman = Some("sniper".into());
    s.args = vec!["-novid".into()];
    s.wrappers = vec!["gamescope -f --".into()];
    let mut command = CommandSpec {
        program: "/games/Game/bin/game.x86_64".into(),
        args: vec!["-steam".into()],
        ..CommandSpec::default()
    };
    apply(&s, &mut command);
    assert_eq!(command.program, std::path::Path::new("gamescope"));
    assert_eq!(
        command.args,
        [
            "-f",
            "--",
            SRUMAN,
            "execute",
            "-p",
            "sniper",
            "--wait",
            "/games/Game/bin/game.x86_64",
            "--",
            "-steam",
            "-novid",
        ]
    );
}

#[test]
fn sruman_is_found_in_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (empty, bin) = (dir.path().join("empty"), dir.path().join("bin"));
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    let path = std::env::join_paths([&empty, &bin]).unwrap();
    assert_eq!(find_in_path(SRUMAN, Some(&path)), None);

    let exe = bin.join(SRUMAN);
    std::fs::write(&exe, "#!/bin/sh\n").unwrap();
    assert_eq!(find_in_path(SRUMAN, Some(&path)), None, "not executable");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(find_in_path(SRUMAN, Some(&path)), Some(exe));
    assert_eq!(find_in_path(SRUMAN, None), None);
}
