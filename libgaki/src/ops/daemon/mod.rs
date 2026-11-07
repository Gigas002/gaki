//! Running `gaki-cli` processes: finding them, telling the session daemon
//! from one-off commands, and stopping them. Backs `gaki-cli kill`,
//! `gaki-cli daemon stop|list` and the TUI's Daemons tab.

use sysinfo::{Pid, System};

/// The CLI's executable name.
pub const CLI_NAME: &str = "gaki-cli";

/// Global flags that take a value as the next argument.
const VALUE_FLAGS: &[&str] = &["--log", "--config-dir", "--socket"];

/// A running `gaki-cli` process other than the current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GakiProcess {
    pub pid: u32,
    /// Whether this is a session daemon (`gaki-cli daemon`) rather than a
    /// thin client or one-off command.
    pub is_daemon: bool,
    /// The full command line, for display.
    pub command: String,
    /// `--config-dir`, when given.
    pub config_dir: Option<String>,
    /// `--socket`, when given.
    pub socket: Option<String>,
}

/// The value of global flag `flag` in `argv` (`--flag value` or
/// `--flag=value`).
fn flag_value(argv: &[String], flag: &str) -> Option<String> {
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().cloned();
        }
        if let Some(value) = arg.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(value.to_string());
        }
    }
    None
}

/// Whether a `gaki-cli` command line runs the session daemon: its only
/// positional argument is `daemon`, so `daemon stop|list` and other
/// commands don't count. Flags go anywhere (they're global); the values of
/// the value-taking flags (`--log`, `--config-dir`, `--socket`) are skipped.
pub fn is_daemon_command(argv: &[String]) -> bool {
    let mut positional = Vec::new();
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            positional.extend(args.by_ref());
            break;
        }
        if VALUE_FLAGS.contains(&arg.as_str()) {
            args.next();
        } else if !arg.starts_with('-') {
            positional.push(arg);
        }
    }
    positional.len() == 1 && positional[0] == "daemon"
}

/// Every running `gaki-cli` process except the current one, by PID.
pub fn find_gaki_processes() -> Vec<GakiProcess> {
    let me = std::process::id();
    let Ok(sys) = System::new_all() else {
        return Vec::new();
    };
    let mut out: Vec<GakiProcess> = sys
        .processes()
        .iter()
        .filter(|(pid, _)| pid.as_u32() != me)
        .filter(|(_, p)| p.name().to_string_lossy() == CLI_NAME)
        .map(|(pid, p)| {
            let argv: Vec<String> = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            GakiProcess {
                pid: pid.as_u32(),
                is_daemon: is_daemon_command(&argv),
                config_dir: flag_value(&argv, "--config-dir"),
                socket: flag_value(&argv, "--socket"),
                command: argv.join(" "),
            }
        })
        .collect();
    out.sort_by_key(|p| p.pid);
    out
}

/// The running session daemons.
pub fn daemons() -> Vec<GakiProcess> {
    find_gaki_processes()
        .into_iter()
        .filter(|p| p.is_daemon)
        .collect()
}

/// Terminate the given PIDs; returns how many were signalled.
pub fn kill_pids(pids: &[u32]) -> usize {
    if pids.is_empty() {
        return 0;
    }
    let Ok(sys) = System::new_all() else {
        return 0;
    };
    pids.iter()
        .filter_map(|&pid| sys.process(Pid::from_u32(pid)))
        .filter(|p| p.kill())
        .count()
}

#[cfg(test)]
mod tests;
