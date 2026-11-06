//! Discovery and termination of running `gaki-cli` processes, backing the
//! `gaki-cli kill` and `gaki-cli daemon stop|list` commands.

use sysinfo::{Pid, System};

/// A running `gaki-cli` process other than the current one.
pub struct GakiProcess {
    pub pid: u32,
    /// Whether this is a session daemon (`gaki-cli daemon`) rather than a thin client
    /// or one-off command.
    pub is_daemon: bool,
    /// The full command line, for display.
    pub command: String,
}

/// True if a process executable name is this binary's (`gaki-cli`).
fn is_gaki_cli_name(name: &str) -> bool {
    name == env!("CARGO_BIN_NAME")
}

/// Decide whether a command line is the session daemon: bare `daemon`, possibly
/// after global flags (`--config-dir ... --socket ... daemon`). Parsed with the
/// real CLI definition, so `daemon stop|list` and other commands don't match.
fn cmd_is_daemon(cmd: &[String]) -> bool {
    use clap::Parser;
    matches!(
        crate::cli::Cli::try_parse_from(cmd),
        Ok(crate::cli::Cli {
            command: crate::cli::Command::Daemon { command: None },
            ..
        })
    )
}

/// Enumerate running `gaki-cli` processes, excluding the current process.
pub fn find_gaki_processes() -> Vec<GakiProcess> {
    let me = std::process::id();
    let Ok(sys) = System::new_all() else {
        return Vec::new();
    };

    let mut out: Vec<GakiProcess> = sys
        .processes()
        .iter()
        .filter(|(pid, _)| pid.as_u32() != me)
        .filter(|(_, p)| is_gaki_cli_name(&p.name().to_string_lossy()))
        .map(|(pid, p)| {
            let cmd: Vec<String> = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            GakiProcess {
                pid: pid.as_u32(),
                is_daemon: cmd_is_daemon(&cmd),
                command: cmd.join(" "),
            }
        })
        .collect();
    out.sort_by_key(|p| p.pid);
    out
}

/// Terminate the given pids, returning how many were successfully signalled.
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
