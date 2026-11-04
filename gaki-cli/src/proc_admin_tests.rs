use super::*;

#[test]
fn recognises_its_own_executable_name() {
    assert!(is_aurelia_name("gaki-cli"));
    assert!(!is_aurelia_name("gaki-cli-helper"));
    assert!(!is_aurelia_name("aurelia"));
    assert!(!is_aurelia_name("steam"));
}

#[test]
fn distinguishes_daemon_from_admin_and_clients() {
    let s = |args: &[&str]| args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    assert!(cmd_is_daemon(&s(&["gaki-cli", "daemon"])));
    assert!(cmd_is_daemon(&s(&[
        "gaki-cli",
        "--config-dir",
        "/c",
        "--socket",
        "/tmp/x",
        "daemon"
    ])));
    assert!(cmd_is_daemon(&s(&[
        "gaki-cli", "daemon", "--socket", "/tmp/x"
    ])));
    // A `daemon stop|list` invocation is not itself a daemon.
    assert!(!cmd_is_daemon(&s(&["aurelia", "daemon", "stop"])));
    assert!(!cmd_is_daemon(&s(&["aurelia", "daemon", "list"])));
    // Thin clients / one-off commands.
    assert!(!cmd_is_daemon(&s(&["aurelia", "list"])));
    assert!(!cmd_is_daemon(&s(&["aurelia"])));
}
