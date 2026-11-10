//! `launch` command handlers.

use crate::commands::common::*;

use crate::cli::PlatformArg;
use anyhow::{Result, bail};
use libgaki::ops::launch::{self, LaunchEvent, PlayRequest};

pub(crate) async fn cmd_launch_targets(app_id: u32, steam: bool) -> Result<()> {
    let mut client = authed_client().await?;
    let launch::Targets { game, targets } = launch::targets(&mut client, app_id, steam).await?;

    if targets.is_empty() {
        cli_println!("{} has no Linux or Windows launch entries.", game.name);
        return Ok(());
    }
    cli_println!("Launch targets for {} (* = default):", game.name);
    for t in &targets {
        let mark = if t.default { "*" } else { " " };
        match t.missing {
            None => cli_println!(
                " {mark} {:<20} gaki-cli play {app_id} {}",
                t.id,
                t.flags.join(" ")
            ),
            Some(missing) => cli_println!(" {mark} {:<20} (needs {missing})", t.id),
        }
    }
    Ok(())
}

pub(crate) async fn cmd_play(
    app_id: u32,
    proton: Option<String>,
    steam: bool,
    platform: Option<PlatformArg>,
    no_update: bool,
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
            "{} is not installed — install it with `gaki-cli install {app_id}`",
            game.name
        );
    }

    let request = PlayRequest {
        proton,
        steam,
        platform: platform.map(Into::into),
        no_update,
    };
    let name = game.name.clone();
    let on_event = |event: LaunchEvent<'_>| match event {
        LaunchEvent::DrmUnverified { error } => {
            let warning = format!(
                "Steam issued no ownership ticket for {name} ({error:#}). If the game uses \
                 Steam DRM it may not start standalone — try `gaki-cli play {app_id} --steam`."
            );
            cli_println!("⚠ {warning}");
        }
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
        drive_progress_until(rx)
    })
    .await?;
    cli_println!("Finished playing {}.", name);

    Ok(())
}

/// `gaki-cli running`: list the games gaki currently has running (stale records
/// whose process has exited are pruned).
pub(crate) fn cmd_running() -> Result<()> {
    let running = launch::running();
    if running.is_empty() {
        cli_println!("No games are running (none launched via `gaki-cli play`).");
    } else {
        cli_println!("{:>9}  {:>8}  NAME", "APPID", "PID");
        for g in &running {
            cli_println!("{:>9}  {:>8}  {}", g.app_id, g.pid, g.name);
        }
    }
    Ok(())
}

pub(crate) async fn cmd_stop(app_id: Option<u32>, force: bool) -> Result<()> {
    // No app id: report what gaki currently tracks as running.
    let Some(app_id) = app_id else {
        return cmd_running();
    };

    let stopped = launch::stop(app_id, force)?;
    let how = if force { " (forced)" } else { "" };
    cli_println!("Stopped {} (app {}){}.", stopped.name, stopped.app_id, how);

    Ok(())
}
