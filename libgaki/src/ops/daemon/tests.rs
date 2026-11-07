use super::*;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

#[test]
fn tells_the_daemon_from_admin_and_clients() {
    assert!(is_daemon_command(&argv(&["gaki-cli", "daemon"])));
    assert!(is_daemon_command(&argv(&[
        "gaki-cli",
        "--config-dir",
        "/c",
        "--socket",
        "/tmp/x",
        "daemon"
    ])));
    assert!(is_daemon_command(&argv(&[
        "gaki-cli", "daemon", "--socket", "/tmp/x"
    ])));
    assert!(is_daemon_command(&argv(&[
        "gaki-cli",
        "--log=debug",
        "-vv",
        "daemon"
    ])));
    assert!(!is_daemon_command(&argv(&["gaki-cli", "daemon", "stop"])));
    assert!(!is_daemon_command(&argv(&["gaki-cli", "daemon", "list"])));
    assert!(!is_daemon_command(&argv(&["gaki-cli", "list"])));
    assert!(!is_daemon_command(&argv(&["gaki-cli"])));
    assert!(
        !is_daemon_command(&argv(&["gaki-cli", "--config-dir", "daemon", "list"])),
        "a flag value named `daemon` isn't the command"
    );
}

#[test]
fn reads_flag_values_both_ways() {
    let a = argv(&["gaki-cli", "--config-dir", "/c", "--socket=/s", "daemon"]);
    assert_eq!(flag_value(&a, "--config-dir").as_deref(), Some("/c"));
    assert_eq!(flag_value(&a, "--socket").as_deref(), Some("/s"));
    assert_eq!(flag_value(&a, "--log"), None);
}
