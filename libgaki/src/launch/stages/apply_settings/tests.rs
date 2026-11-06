use std::collections::BTreeMap;

use super::*;

fn settings() -> LaunchSettings {
    LaunchSettings {
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
