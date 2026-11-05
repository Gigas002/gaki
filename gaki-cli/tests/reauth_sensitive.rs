//! `reauth_sensitive`: the inventory, wallet and market listings ask for a
//! fresh login first, before anything reaches Steam.

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::{TempDir, tempdir};

fn config_dir() -> TempDir {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("config.toml"),
        "secret_store = \"memory\"\nreauth_sensitive = true\n",
    )
    .unwrap();
    tmp
}

fn run(config_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gaki-cli"))
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--no-daemon")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed running the gaki-cli binary")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn json_mode_cannot_sign_in_again() {
    let tmp = config_dir();
    for args in [
        &["--json", "wallet"][..],
        &["--json", "inventory", "440"],
        &["--json", "market", "listings"],
    ] {
        let out = run(tmp.path(), args);
        assert!(!out.status.success(), "{args:?} should fail");
        // `--json` errors are a typed line on stdout.
        let err = String::from_utf8_lossy(&out.stdout);
        assert!(
            err.contains("reauth_sensitive is on") && err.contains("auth_required"),
            "{args:?}: unexpected error: {err}"
        );
    }
}

#[test]
fn interactive_mode_asks_for_a_login() {
    let tmp = config_dir();
    let out = run(tmp.path(), &["wallet"]);
    // No terminal to prompt on, so it stops at the prompt.
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("sign in again to continue"), "{err}");
}

#[test]
fn price_lookups_stay_open() {
    let tmp = config_dir();
    // Search needs no login, so it is not gated: whatever happens (it may
    // have no network here), it never asks for one.
    let out = run(tmp.path(), &["--json", "market", "search", "--count", "1"]);
    assert!(
        !stderr(&out).contains("reauth_sensitive"),
        "{}",
        stderr(&out)
    );
}
