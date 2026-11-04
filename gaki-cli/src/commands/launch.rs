//! `launch` command handlers.

use crate::proc_admin;

use crate::commands::common::*;

use crate::cli::PlatformArg;
use anyhow::{Result, bail};
use libgaki::ops::launch::{self, LaunchEvent, PlayRequest};
use std::path::PathBuf;

pub(crate) async fn cmd_launch_targets(app_id: u32, steam: bool, json: bool) -> Result<()> {
    let mut client = authed_client().await?;
    let launch::Targets { game, targets } = launch::targets(&mut client, app_id, steam).await?;

    if json {
        let arr: Vec<_> = targets
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "platform": t.platform,
                    "steam_mode": t.steam_mode,
                    "flags": t.flags,
                    "available": t.available,
                    "missing": t.missing,
                    "default": t.default,
                })
            })
            .collect();
        print_json(&serde_json::json!({ "app_id": app_id, "targets": arr }));
        return Ok(());
    }

    if targets.is_empty() {
        cli_println!("{} has no Linux or Windows launch entries.", game.name);
        return Ok(());
    }
    cli_println!("Launch targets for {} (* = default):", game.name);
    for t in &targets {
        let mark = if t.default { "*" } else { " " };
        match t.missing {
            None => cli_println!(
                " {mark} {:<20} aurelia play {app_id} {}",
                t.id,
                t.flags.join(" ")
            ),
            Some(missing) => cli_println!(" {mark} {:<20} (needs {missing})", t.id),
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per CLI flag; replaced by resolved settings in Phase 5"
)]
pub(crate) async fn cmd_play(
    app_id: u32,
    proton: Option<String>,
    script: Option<PathBuf>,
    no_script: bool,
    steam: bool,
    platform: Option<PlatformArg>,
    no_update: bool,
    json: bool,
) -> Result<()> {
    let linux_target = matches!(platform, Some(PlatformArg::Linux));
    if linux_target && proton.is_some() {
        bail!("--platform linux runs the native build; it can't be combined with --proton");
    }
    let mut client = authed_client().await?;
    let game = find_game(&mut client, app_id).await?;

    // Otherwise the spawn fails on the missing directory with a raw OS error.
    if !game.is_installed {
        bail!(
            "{} is not installed — install it with `aurelia install {app_id}`",
            game.name
        );
    }

    let request = PlayRequest {
        proton,
        script,
        no_script,
        steam,
        platform: platform.map(Into::into),
        no_update,
    };
    let name = game.name.clone();
    let on_event = |event: LaunchEvent<'_>| match event {
        LaunchEvent::DrmUnverified { error } => {
            let warning = format!(
                "Steam issued no ownership ticket for {name} ({error:#}). If the game uses \
                 Steam DRM it may not start standalone — try `aurelia play {app_id} --steam`."
            );
            if json {
                tracing::warn!(appid = app_id, "{warning}");
            } else {
                cli_println!("⚠ {warning}");
            }
        }
        _ if json => {}
        LaunchEvent::Updating { required } => {
            let kind = if required {
                "Required update"
            } else {
                "Update available"
            };
            cli_println!("{kind} for {name} — installing ...");
        }
        LaunchEvent::Launching => cli_println!("Launching {name} ..."),
        LaunchEvent::Target { proton: Some(p) } => {
            cli_println!("Platform: Windows via Proton ({p})")
        }
        LaunchEvent::Target { proton: None } => cli_println!("Platform: native Linux"),
    };
    launch::play(&mut client, game, &request, &on_event, |rx| {
        drive_progress_until(rx, json)
    })
    .await?;
    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "name": name,
            "status": "finished",
        }));
    } else {
        cli_println!("Finished playing {}.", name);
    }
    Ok(())
}

/// `aurelia running`: list the games Aurelia currently has running (stale records
/// whose process has exited are pruned).
pub(crate) fn cmd_running(json: bool) -> Result<()> {
    let running = launch::running();
    if json {
        let arr: Vec<_> = running
            .iter()
            .map(|g| serde_json::json!({ "app_id": g.app_id, "name": g.name, "pid": g.pid }))
            .collect();
        print_json(&serde_json::json!({ "running": arr }));
        return Ok(());
    }
    if running.is_empty() {
        cli_println!("No games are running (none launched via `aurelia play`).");
    } else {
        cli_println!("{:>9}  {:>8}  NAME", "APPID", "PID");
        for g in &running {
            cli_println!("{:>9}  {:>8}  {}", g.app_id, g.pid, g.name);
        }
    }
    Ok(())
}

pub(crate) async fn cmd_stop(app_id: Option<u32>, force: bool, json: bool) -> Result<()> {
    // No app id: report what Aurelia currently tracks as running.
    let Some(app_id) = app_id else {
        return cmd_running(json);
    };

    let stopped = launch::stop(app_id, force)?;
    if json {
        print_json(&serde_json::json!({
            "app_id": stopped.app_id,
            "name": stopped.name,
            "status": "stopped",
            "forced": force,
        }));
    } else {
        let how = if force { " (forced)" } else { "" };
        cli_println!("Stopped {} (app {}){}.", stopped.name, stopped.app_id, how);
    }
    Ok(())
}

/// `aurelia kill`: terminate every running aurelia process (daemon and otherwise),
/// except the current one.
pub(crate) fn cmd_kill(json: bool) -> Result<()> {
    let procs = proc_admin::find_aurelia_processes();
    let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
    let killed = proc_admin::kill_pids(&pids);

    if json {
        print_json(&serde_json::json!({ "found": pids.len(), "killed": killed, "pids": pids }));
    } else if pids.is_empty() {
        cli_println!("No other aurelia processes are running.");
    } else {
        cli_println!(
            "Killed {killed} of {} aurelia process(es) (including the daemon).",
            pids.len()
        );
    }
    Ok(())
}

/// `aurelia daemon stop [PID]`: terminate the session daemon(s). With a PID, stop
/// only that daemon (erroring if it isn't a running aurelia daemon).
pub(crate) fn cmd_daemon_stop(pid: Option<u32>, json: bool) -> Result<()> {
    let daemons: Vec<_> = proc_admin::find_aurelia_processes()
        .into_iter()
        .filter(|p| p.is_daemon)
        .collect();

    let targets: Vec<u32> = match pid {
        Some(pid) => {
            if !daemons.iter().any(|d| d.pid == pid) {
                bail!("PID {pid} is not a running aurelia daemon (see `aurelia daemon list`)");
            }
            vec![pid]
        }
        None => daemons.iter().map(|d| d.pid).collect(),
    };
    let killed = proc_admin::kill_pids(&targets);

    if json {
        print_json(&serde_json::json!({ "killed": killed, "pids": targets }));
    } else if targets.is_empty() {
        cli_println!("No aurelia daemon is running.");
    } else {
        cli_println!("Stopped {killed} aurelia daemon(s).");
    }
    Ok(())
}

/// `aurelia daemon list`: show running aurelia daemon(s) and their PIDs.
pub(crate) fn cmd_daemon_list(json: bool) -> Result<()> {
    let daemons: Vec<_> = proc_admin::find_aurelia_processes()
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
        cli_println!("No aurelia daemon is running.");
        return Ok(());
    }
    cli_println!("{:>8}  COMMAND", "PID");
    for d in &daemons {
        cli_println!("{:>8}  {}", d.pid, d.command);
    }
    Ok(())
}
