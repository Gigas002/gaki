//! `gaki-cli login` must warn about an already-active session. The warning
//! reads `account.toml` only, never the keyring.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

// No TTY here: login fails at the password
// prompt, before any Steam contact.
fn login(config_dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gaki-cli"))
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--no-daemon")
        .arg("login")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed running the gaki-cli binary")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// A config dir that keeps secrets in memory, so no test touches the keyring.
fn config_dir() -> tempfile::TempDir {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("config.toml"),
        "secret_store = \"memory\"\n",
    )
    .unwrap();
    tmp
}

fn with_account(toml: &str) -> tempfile::TempDir {
    let tmp = config_dir();
    fs::write(tmp.path().join("account.toml"), toml).unwrap();
    tmp
}

#[test]
fn a_session_prefers_the_persona_name() {
    let tmp = with_account(
        "account_name = \"tester_acct\"\npersona_name = \"Tester\"\nsteam_id = 1\nrefresh_token = true\n",
    );
    let err = stderr(&login(tmp.path()));
    assert!(
        err.contains("already logged in as Tester"),
        "missing warning: {err}"
    );
}

#[test]
fn a_session_falls_back_to_the_account_name() {
    let tmp = with_account("account_name = \"tester_acct\"\nsteam_id = 1\nrefresh_token = true\n");
    let err = stderr(&login(tmp.path()));
    assert!(
        err.contains("already logged in as tester_acct"),
        "missing warning: {err}"
    );
}

#[test]
fn a_nameless_session_warns_without_a_name() {
    let tmp = with_account("steam_id = 1\nweb_token = true\n");
    let err = stderr(&login(tmp.path()));
    assert!(
        err.contains("an existing session is active"),
        "missing warning: {err}"
    );
}

#[test]
fn no_session_prints_no_warning() {
    let tmp = config_dir();
    let err = stderr(&login(tmp.path()));
    assert!(!err.contains("Warning:"), "unexpected warning: {err}");
}

#[test]
fn empty_account_prints_no_warning() {
    let tmp = with_account("");
    let err = stderr(&login(tmp.path()));
    assert!(!err.contains("Warning:"), "unexpected warning: {err}");
}
