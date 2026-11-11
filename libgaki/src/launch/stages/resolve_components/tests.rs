use super::*;
use crate::core::models::{LaunchInfo, LaunchTarget};
use crate::core::models::{LibraryGame, UserAppConfig};

#[tokio::test]
async fn native_runner_appends_user_launch_options() {
    let user_config = UserAppConfig {
        launch_options: "-fullscreen --skip-intro".to_string(),
        ..Default::default()
    };

    let ctx = LaunchContext {
        app: LibraryGame {
            app_id: 123,
            name: "Test".to_string(),
            install_path: Some("/tmp/game".to_string()),
            is_installed: true,
            playtime_forever_minutes: None,
            playtime_2weeks_minutes: None,
            last_played: None,
            active_branch: "public".to_string(),
            update_available: false,
            update_queued: false,
            local_manifest_ids: HashMap::new(),
            is_owned: true,
            is_family_shared: false,
            online_required: None,
            platform: None,
            size_on_disk: None,
        },
        launch_info: LaunchInfo {
            app_id: 123,
            id: "0".into(),
            description: "Test".into(),
            executable: "game.bin".into(),
            arguments: "-v".into(),
            workingdir: None,
            target: LaunchTarget::NativeLinux,
        },
        launcher_config: crate::core::config::LauncherConfig::default(),
        user_config: Some(user_config),
        proton_path: None,
        settings: None,
        steam_enabled: false,
        target_architecture: Default::default(),
        dll_resolutions: Vec::new(),
        game_fixups: Default::default(),
        verification: Default::default(),
    };

    let spec = NativeRunner.build_command(&ctx).await.unwrap();
    assert_eq!(spec.args, vec!["-v", "-fullscreen", "--skip-intro"]);
}

/// Writes `bytes` to `dir/name`, executable.
fn write_exec(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

const ELF32: &[u8] = b"\x7fELF\x01\x01\x01\0rest";
const ELF64: &[u8] = b"\x7fELF\x02\x01\x01\0rest";

#[test]
fn scout_libraries_are_for_32_bit_natives() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(needs_scout_libs(&write_exec(dir, "game32", ELF32), dir));
    assert!(!needs_scout_libs(&write_exec(dir, "game64", ELF64), dir));
    assert!(!needs_scout_libs(&dir.join("missing"), dir));
    // A launcher script counts when a 32-bit binary sits beside it.
    let script = write_exec(dir, "hl.sh", b"#!/bin/sh\nexec ./game32\n");
    assert!(needs_scout_libs(&script, dir));

    let other = tempfile::tempdir().unwrap();
    let script = write_exec(other.path(), "run.sh", b"#!/bin/sh\n");
    write_exec(other.path(), "game64", ELF64);
    assert!(!needs_scout_libs(&script, other.path()));
    // Not a script, not ELF: never.
    let text = write_exec(other.path(), "notes", b"hello");
    write_exec(other.path(), "game32", ELF32);
    assert!(!needs_scout_libs(&text, other.path()));
}

#[tokio::test]
async fn the_stage_picks_the_runner_for_the_target() {
    let info = |target| LaunchInfo {
        app_id: 1,
        id: "0".into(),
        description: String::new(),
        executable: "x".into(),
        arguments: String::new(),
        workingdir: None,
        target,
    };
    for (target, name) in [
        (LaunchTarget::NativeLinux, "Native"),
        (LaunchTarget::WindowsProton, "Wine-TKG"),
    ] {
        let mut ctx = PipelineContext::new(1);
        ctx.launch_info = Some(info(target));
        ResolveComponentsStage.execute(&mut ctx).await.unwrap();
        assert_eq!(ctx.runner.as_ref().unwrap().name(), name);
    }

    // A runner chosen earlier is kept; no launch info is an error.
    let mut ctx = PipelineContext::new(1);
    ctx.runner = Some(Box::new(NativeRunner));
    ResolveComponentsStage.execute(&mut ctx).await.unwrap();
    assert_eq!(ctx.runner.as_ref().unwrap().name(), "Native");
    let mut ctx = PipelineContext::new(1);
    let err = ResolveComponentsStage.execute(&mut ctx).await.unwrap_err();
    assert_eq!(err.kind, LaunchErrorKind::Validation);
}

#[test]
fn native_launch_runs_scripts_without_a_shebang_and_logs_to_the_session() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_exec(tmp.path(), "start", b"echo out-$1\necho err >&2\n");
    let out = tmp.path().join("logs/stdout.log");
    let err = tmp.path().join("logs/stderr.log");
    let spec = CommandSpec {
        program: script,
        args: vec!["one".into()],
        cwd: Some(tmp.path().to_path_buf()),
        env: HashMap::from([
            (
                crate::launch::game_env::ENV_STDOUT_LOG.to_string(),
                out.to_string_lossy().into_owned(),
            ),
            (
                crate::launch::game_env::ENV_STDERR_LOG.to_string(),
                err.to_string_lossy().into_owned(),
            ),
        ]),
    };
    let status = NativeRunner.launch(&spec).unwrap().wait().unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "out-one\n");
    assert_eq!(std::fs::read_to_string(&err).unwrap(), "err\n");

    let missing = CommandSpec {
        program: tmp.path().join("nope"),
        ..Default::default()
    };
    assert_eq!(
        NativeRunner.launch(&missing).unwrap_err().kind,
        LaunchErrorKind::Process
    );
}
