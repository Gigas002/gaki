//! The stored session's error surfacing: `account.toml` plus the secret
//! store. Every config dir sets `secret_store = "memory"`, so no test touches
//! the real keyring and each process starts with no secrets.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::{TempDir, tempdir};

fn gaki() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gaki-cli"));
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// A config dir that keeps secrets in memory.
fn config_dir() -> TempDir {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("config.toml"),
        "secret_store = \"memory\"\n",
    )
    .unwrap();
    tmp
}

fn gaki_cli(config_dir: &Path, args: &[&str]) -> Output {
    gaki()
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--no-daemon")
        .args(args)
        .output()
        .expect("failed running the gaki-cli binary")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// An account whose refresh token the store doesn't have: a keyring that
/// lost it, or a memory store in a new process.
fn with_lost_token() -> TempDir {
    let tmp = config_dir();
    fs::write(
        tmp.path().join("account.toml"),
        "account_name = \"tester\"\nsteam_id = 76561198000000001\nrefresh_token = true\n",
    )
    .unwrap();
    tmp
}

#[test]
fn a_lost_token_asks_for_a_login() {
    let tmp = with_lost_token();
    let out = gaki_cli(tmp.path(), &["account"]);
    assert!(!out.status.success(), "expected failure, got: {out:?}");
    let err = stderr(&out);
    assert!(
        err.contains("refresh-token") && err.contains("gaki-cli login"),
        "error must say what is missing: {err}"
    );
    assert!(
        !err.contains("not logged in"),
        "must not misreport as logged out: {err}"
    );
}

#[test]
fn an_old_session_json_asks_for_a_login() {
    let tmp = config_dir();
    fs::write(
        tmp.path().join("session.json"),
        r#"{"account_name":"tester","refresh_token":"tok"}"#,
    )
    .unwrap();
    let out = gaki_cli(tmp.path(), &["account"]);
    assert!(!out.status.success(), "expected failure, got: {out:?}");
    let err = stderr(&out);
    assert!(
        err.contains("session.json") && err.contains("keyring") && err.contains("gaki-cli login"),
        "error must explain the move: {err}"
    );
}

#[test]
fn logout_clears_the_account() {
    let tmp = with_lost_token();
    fs::write(tmp.path().join("session.json"), "{}").unwrap();
    let out = gaki_cli(tmp.path(), &["logout"]);
    assert!(out.status.success(), "logout failed: {}", stderr(&out));
    assert!(!tmp.path().join("account.toml").exists());
    assert!(!tmp.path().join("session.json").exists());
    // The token couldn't be read, so Steam couldn't revoke it: say so.
    let err = stderr(&out);
    assert!(
        err.contains("could not revoke") && err.contains("until it expires"),
        "missing revocation warning: {err}"
    );
}

#[test]
fn logout_without_a_session_has_nothing_to_revoke() {
    let tmp = config_dir();
    let out = gaki_cli(tmp.path(), &["--json", "logout"]);
    assert!(out.status.success(), "logout failed: {}", stderr(&out));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["logged_out"], true);
    assert_eq!(json["token"]["revocation"], "no_token", "{json}");
}

mod daemon_e2e {
    use super::*;
    use std::time::{Duration, Instant};

    // Daemon subprocess; killed on drop.
    struct DaemonProc(std::process::Child);

    impl Drop for DaemonProc {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn spawn_daemon(config_dir: &Path, socket: &Path) -> DaemonProc {
        let child = gaki()
            .arg("--config-dir")
            .arg(config_dir)
            .arg("--socket")
            .arg(socket)
            .arg("daemon")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed spawning the daemon");
        // Marker too, so clients skip mismatch handling.
        let marker = socket.with_extension("info");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !socket.exists() || !marker.exists() {
            assert!(Instant::now() < deadline, "daemon socket never appeared");
            std::thread::sleep(Duration::from_millis(50));
        }
        DaemonProc(child)
    }

    fn account_via(config_dir: &Path, socket: &Path) -> Output {
        gaki()
            .arg("--config-dir")
            .arg(config_dir)
            .arg("--socket")
            .arg(socket)
            .args(["--no-spawn", "account"])
            .output()
            .expect("failed running the gaki-cli binary")
    }

    #[test]
    fn logged_out_stays_a_plain_not_logged_in() {
        let tmp = config_dir();
        let socket = tmp.path().join("daemon.sock");
        let _daemon = spawn_daemon(tmp.path(), &socket);
        let out = account_via(tmp.path(), &socket);
        assert!(!out.status.success(), "expected failure, got: {out:?}");
        assert!(
            stderr(&out).contains("not logged in"),
            "a missing session is simply logged out: {}",
            stderr(&out)
        );
    }

    #[test]
    fn daemon_surfaces_the_restore_error() {
        let tmp = with_lost_token();
        let socket = tmp.path().join("daemon.sock");
        let _daemon = spawn_daemon(tmp.path(), &socket);
        let out = account_via(tmp.path(), &socket);
        assert!(!out.status.success(), "expected failure, got: {out:?}");
        let err = stderr(&out);
        assert!(
            err.contains("could not restore the stored session") || err.contains("refresh-token"),
            "unexpected error: {err}"
        );
        assert!(
            !err.contains("not logged in"),
            "must not misreport as logged out: {err}"
        );
    }
}
