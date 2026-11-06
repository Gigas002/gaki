//! Session daemon and process admin: `daemon stop|list`, `kill`.

use crate::proc_admin;

use crate::commands::common::*;

use anyhow::{Result, bail};

/// `gaki-cli kill`: terminate every running gaki-cli process (daemon and otherwise),
/// except the current one.
pub(crate) fn cmd_kill(json: bool) -> Result<()> {
    let procs = proc_admin::find_gaki_processes();
    let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
    let killed = proc_admin::kill_pids(&pids);

    if json {
        print_json(&serde_json::json!({ "found": pids.len(), "killed": killed, "pids": pids }));
    } else if pids.is_empty() {
        cli_println!("No other gaki-cli processes are running.");
    } else {
        cli_println!(
            "Killed {killed} of {} gaki-cli process(es) (including the daemon).",
            pids.len()
        );
    }
    Ok(())
}

/// `gaki-cli daemon stop [PID]`: terminate the session daemon(s). With a PID, stop
/// only that daemon (erroring if it isn't a running gaki-cli daemon).
pub(crate) fn cmd_daemon_stop(pid: Option<u32>, json: bool) -> Result<()> {
    let daemons: Vec<_> = proc_admin::find_gaki_processes()
        .into_iter()
        .filter(|p| p.is_daemon)
        .collect();

    let targets: Vec<u32> = match pid {
        Some(pid) => {
            if !daemons.iter().any(|d| d.pid == pid) {
                bail!("PID {pid} is not a running gaki-cli daemon (see `gaki-cli daemon list`)");
            }
            vec![pid]
        }
        None => daemons.iter().map(|d| d.pid).collect(),
    };
    let killed = proc_admin::kill_pids(&targets);

    if json {
        print_json(&serde_json::json!({ "killed": killed, "pids": targets }));
    } else if targets.is_empty() {
        cli_println!("No gaki-cli daemon is running.");
    } else {
        cli_println!("Stopped {killed} gaki-cli daemon(s).");
    }
    Ok(())
}

/// `gaki-cli daemon list`: show running gaki-cli daemon(s) and their PIDs.
pub(crate) fn cmd_daemon_list(json: bool) -> Result<()> {
    let daemons: Vec<_> = proc_admin::find_gaki_processes()
        .into_iter()
        .filter(|p| p.is_daemon)
        .collect();

    if json {
        let arr: Vec<_> = daemons
            .iter()
            .map(|d| serde_json::json!({ "pid": d.pid, "command": d.command }))
            .collect();
        print_json(&serde_json::json!({ "daemons": arr }));
        return Ok(());
    }

    if daemons.is_empty() {
        cli_println!("No gaki-cli daemon is running.");
        return Ok(());
    }
    cli_println!("{:>8}  COMMAND", "PID");
    for d in &daemons {
        cli_println!("{:>8}  {}", d.pid, d.command);
    }
    Ok(())
}
