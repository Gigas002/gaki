//! A corrupt `config.toml` or `config.json` must never be silently replaced by
//! defaults.
//!
//! `load_launcher_config` already returns `Ok(defaults)` when the file is *missing*, so
//! an `Err` only ever means "the file exists but could not be parsed". Callers used to
//! paper over that with `.unwrap_or_default()`, which meant a single typo made every
//! setting vanish — and any command that then saved wrote those defaults straight over
//! the user's file.
//!
//! These drive the real binary as a subprocess, each with its own `--config-dir`.
//! `--no-daemon` stops the CLI from forwarding to a session daemon (which serves
//! its own config directory, not ours).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::{TempDir, tempdir};

const CORRUPT: &str = r#"{
  "steam_library_path": "/mnt/games/SteamLibrary",
  "proton_version": "GE-Proton9-20",
  "enable_cloud_sync": yes,
  "preferred_launch_options": { "570": "-novid -high" },
  "chat_presence": "online"
}"#;

fn gaki_cli(config_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gaki-cli"))
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--no-daemon")
        .args(args)
        .output()
        .expect("failed running the gaki-cli binary")
}

fn with_corrupt_config() -> TempDir {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.json"), CORRUPT).unwrap();
    tmp
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// `config.toml` with a syntax error on line 3.
const CORRUPT_TOML: &str = "# settings\ndefault_profile = \"proton\"\ncloud_sync = yes\n";

fn with_corrupt_toml() -> TempDir {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("config.toml"), CORRUPT_TOML).unwrap();
    tmp
}

/// The headline regression: `config presence` mutates and saves. With a corrupt
/// config it must fail and leave the file alone.
#[test]
fn corrupt_config_is_not_overwritten_by_a_saving_command() {
    let tmp = with_corrupt_toml();
    let path = tmp.path().join("config.toml");

    for args in [
        vec!["config", "presence", "online"],
        vec!["config", "set", "experimental", "true"],
    ] {
        let out = gaki_cli(tmp.path(), &args);
        assert!(!out.status.success(), "expected failure, got: {out:?}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            CORRUPT_TOML,
            "`{}` modified the user's config.toml",
            args.join(" ")
        );
    }
}

/// An unparseable config.json must surface as a parse error, not as some
/// downstream symptom of the defaults it would otherwise have been replaced with.
#[test]
fn corrupt_config_reports_a_parse_error() {
    let tmp = with_corrupt_config();
    let err = stderr(&gaki_cli(tmp.path(), &["proton", "default", "Proton 9.0"]));

    assert!(err.contains("invalid launcher config"), "got: {err}");
}

#[test]
fn corrupt_config_error_names_the_file_and_location() {
    let tmp = with_corrupt_config();
    let err = stderr(&gaki_cli(tmp.path(), &["proton", "list"]));
    assert!(err.contains("config.json"), "got: {err}");
    assert!(err.contains("line 4"), "expected a location: {err}");
    assert!(
        err.contains("delete it to regenerate defaults"),
        "expected a suggested fix: {err}"
    );

    let tmp = with_corrupt_toml();
    let err = stderr(&gaki_cli(tmp.path(), &["config", "show"]));
    assert!(err.contains("config.toml"), "got: {err}");
    assert!(err.contains("line 3"), "expected a location: {err}");
}

#[test]
fn corrupt_config_fails_every_reading_command() {
    let tmp = with_corrupt_config();
    for args in [
        vec!["proton", "default", "Proton 9.0"],
        vec!["proton", "list"],
    ] {
        let out = gaki_cli(tmp.path(), &args);
        assert!(
            !out.status.success(),
            "`{}` silently accepted a corrupt config.json",
            args.join(" ")
        );
    }
    let tmp = with_corrupt_toml();
    for args in [
        vec!["config", "show"],
        vec!["config", "get", "cloud_sync"],
        vec!["proton", "list"],
    ] {
        let out = gaki_cli(tmp.path(), &args);
        assert!(
            !out.status.success(),
            "`{}` silently accepted a corrupt config.toml",
            args.join(" ")
        );
    }
}

/// The other half of the contract: a *missing* config is the normal first-run path and
/// must keep yielding defaults. Propagating errors must not break new users.
#[test]
fn missing_config_still_yields_defaults() {
    let tmp = tempdir().unwrap();
    let out = gaki_cli(tmp.path(), &["config", "show"]);

    assert!(
        out.status.success(),
        "fresh install failed: {}",
        stderr(&out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cloud_sync = true"), "got: {stdout}");
}

#[test]
fn missing_config_can_still_be_written() {
    let tmp = tempdir().unwrap();
    let out = gaki_cli(tmp.path(), &["config", "presence", "online"]);
    assert!(out.status.success(), "failed: {}", stderr(&out));

    let written = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
    assert!(written.contains("presence = \"online\""), "got: {written}");
}
