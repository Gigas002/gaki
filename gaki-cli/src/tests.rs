use super::*;

#[cfg(feature = "library")]
#[test]
fn human_bytes_scales_units() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(1024), "1.00 KiB");
    assert_eq!(human_bytes(1536), "1.50 KiB");
    assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.00 GiB");
}

#[cfg(feature = "library")]
#[test]
fn eta_formats_hms() {
    assert_eq!(format_eta(0), "00:00:00");
    assert_eq!(format_eta(83), "00:01:23");
    assert_eq!(format_eta(3661), "01:01:01");
}

/// Parse an argv into a `Cli` the way the binary does.
#[cfg(feature = "authentication")]
fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("args should parse")
}

#[cfg(feature = "install")]
#[test]
fn install_positional_and_subcommands_coexist() {
    // Positional install (Heroic depends on this form).
    match parse(&["gaki-cli", "install", "945360"]).command {
        Command::Install(args) => {
            assert!(args.action.is_none());
            assert_eq!(args.app_id, Some(945360));
        }
        _ => panic!("expected Install"),
    }
    // Management verbs.
    match parse(&["gaki-cli", "install", "list"]).command {
        Command::Install(args) => assert!(matches!(args.action, Some(InstallAction::List))),
        _ => panic!("expected Install list"),
    }
    match parse(&["gaki-cli", "install", "stop", "945360"]).command {
        Command::Install(args) => {
            assert!(matches!(
                args.action,
                Some(InstallAction::Stop { app_id: 945360 })
            ))
        }
        _ => panic!("expected Install stop"),
    }
    match parse(&["gaki-cli", "install", "pause", "945360"]).command {
        Command::Install(args) => {
            assert!(matches!(
                args.action,
                Some(InstallAction::Pause { app_id: 945360 })
            ))
        }
        _ => panic!("expected Install pause"),
    }
    match parse(&["gaki-cli", "install", "resume", "--all"]).command {
        Command::Install(args) => {
            assert!(matches!(
                args.action,
                Some(InstallAction::Resume {
                    app_id: None,
                    all: true
                })
            ))
        }
        _ => panic!("expected Install resume"),
    }
    match parse(&["gaki-cli", "install", "forget", "945360", "--delete-files"]).command {
        Command::Install(args) => {
            assert!(matches!(
                args.action,
                Some(InstallAction::Forget {
                    app_id: 945360,
                    delete_files: true
                })
            ))
        }
        _ => panic!("expected Install forget"),
    }
}

#[cfg(feature = "authentication")]
#[test]
fn interactive_login_runs_locally_not_forwarded() {
    // The regression: these were forwarded to the daemon, where rpassword ran
    // without a tty and the password was echoed in clear text.
    assert!(must_run_locally(&parse(&["gaki-cli", "login"])));
    assert!(must_run_locally(&parse(&["gaki-cli", "login", "-u", "me"])));
    assert!(must_run_locally(&parse(&["gaki-cli", "login", "--qr"])));
    assert!(must_run_locally(&parse(&["gaki-cli", "login", "--code"])));
    assert!(must_run_locally(&parse(&["gaki-cli", "login", "--pin"])));
}

#[cfg(feature = "authentication")]
#[test]
fn daemon_oriented_and_json_login_still_forward() {
    // These need the daemon (or are non-tty), so they must NOT be pinned local.
    assert!(!must_run_locally(&parse(&[
        "gaki-cli", "login", "--health"
    ])));
    assert!(!must_run_locally(&parse(&[
        "gaki-cli",
        "login",
        "--reconnect"
    ])));
    assert!(!must_run_locally(&parse(&[
        "gaki-cli", "--json", "login", "-u", "me"
    ])));
    assert!(!must_run_locally(&parse(&[
        "gaki-cli", "login", "--json", "--qr"
    ])));
}

#[cfg(feature = "authentication")]
#[test]
fn qr_timeout_parses_and_requires_qr() {
    match parse(&["gaki-cli", "login", "--qr", "--timeout", "90"]).command {
        Command::Login(args) => assert_eq!(args.timeout, Some(90)),
        _ => panic!("expected Login"),
    }
    assert!(Cli::try_parse_from(["gaki-cli", "login", "--timeout", "90"]).is_err());
    assert!(Cli::try_parse_from(["gaki-cli", "login", "--qr", "--timeout", "0"]).is_err());
    // Driver QR logins still forward.
    assert!(!must_run_locally(&parse(&[
        "gaki-cli", "--json", "login", "--qr", "-t", "5"
    ])));
}

#[cfg(feature = "library")]
#[test]
fn ordinary_commands_forward_but_local_managers_do_not() {
    assert!(!must_run_locally(&parse(&["gaki-cli", "list"])));
    assert!(!must_run_locally(&parse(&["gaki-cli", "logout"])));
    // Process/daemon managers must run in-process.
    assert!(must_run_locally(&parse(&["gaki-cli", "kill"])));
    assert!(must_run_locally(&parse(&[
        "gaki-cli",
        "--json",
        "capabilities"
    ])));
    assert!(must_run_locally(&parse(&[
        "gaki-cli",
        "capabilities",
        "config",
        "game"
    ])));
    assert!(must_run_locally(&parse(&["gaki-cli", "daemon", "stop"])));
    // Config files only: no session, so no daemon to start.
    assert!(must_run_locally(&parse(&["gaki-cli", "config", "check"])));
    assert!(must_run_locally(&parse(&[
        "gaki-cli", "config", "set", "x", "1"
    ])));
    // Bare `gaki-cli daemon` (becomes the server) is handled earlier, before the
    // forward gate, so it is not flagged local here.
    assert!(!must_run_locally(&parse(&["gaki-cli", "daemon"])));
}

#[test]
fn cli_has_no_conflicting_flags() {
    use clap::CommandFactory;
    crate::cli::Cli::command().debug_assert();
}

#[test]
fn config_dir_is_found_before_parsing() {
    let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        config_dir_arg(args(&["play", "--config-dir", "/x", "witcher3"])),
        Some("/x".into())
    );
    assert_eq!(
        config_dir_arg(args(&["--config-dir=/y", "play", "1"])),
        Some("/y".into())
    );
    assert_eq!(
        config_dir_arg(args(&["play", "--", "--config-dir", "/z"])),
        None
    );
    assert_eq!(config_dir_arg(args(&["play", "1"])), None);
}

/// libgaki's argv rule for spotting the session daemon agrees with this
/// CLI's own parse of the same command lines.
#[cfg(feature = "authentication")]
#[test]
fn the_daemon_argv_rule_matches_clap() {
    use clap::Parser;
    let lines: &[&[&str]] = &[
        &["gaki-cli", "daemon"],
        &[
            "gaki-cli",
            "--config-dir",
            "/c",
            "--socket",
            "/tmp/x",
            "daemon",
        ],
        &["gaki-cli", "daemon", "--socket", "/tmp/x"],
        &["gaki-cli", "--log=debug", "-vv", "daemon"],
        &["gaki-cli", "-j", "daemon"],
        &["gaki-cli", "daemon", "stop"],
        &["gaki-cli", "daemon", "stop", "42"],
        &["gaki-cli", "daemon", "list"],
        &["gaki-cli", "list"],
        &["gaki-cli", "--config-dir", "daemon", "list"],
    ];
    for line in lines {
        let argv: Vec<String> = line.iter().map(|a| a.to_string()).collect();
        let clap_says = matches!(
            cli::Cli::try_parse_from(&argv),
            Ok(cli::Cli {
                command: cli::Command::Daemon { command: None },
                ..
            })
        );
        assert_eq!(
            libgaki::ops::daemon::is_daemon_command(&argv),
            clap_says,
            "{line:?}"
        );
    }
}
