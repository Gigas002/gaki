//! `launch` command handlers.

use crate::proc_admin;

use crate::commands::common::*;

use crate::cli::PlatformArg;
use anyhow::{Context, Result, bail};
use libgaki::core::config::load_launcher_config;
use libgaki::core::config::load_user_configs;
use libgaki::core::models::DownloadState;
use libgaki::steam_client::SteamClient;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub(crate) async fn cmd_launch_targets(app_id: u32, steam: bool, json: bool) -> Result<()> {
    let mut client = authed_client().await?;
    let game = find_game(&mut client, app_id).await?;
    if !game.is_installed {
        bail!("{} is not installed", game.name);
    }
    let options = client
        .get_product_info(app_id)
        .await
        .with_context(|| format!("failed to load launch options for app {app_id}"))?;

    // Mirror `cmd_play` default resolution.
    let launcher_config = load_launcher_config().await?;
    let game_cfg = launcher_config.game_configs.get(&app_id);
    let platform_preference = game_cfg.and_then(|c| c.platform_preference.as_deref());
    let prefer_windows = platform_preference == Some("windows")
        || game_cfg.is_some_and(|c| c.forced_proton_version.is_some());
    let prefer_native = platform_preference == Some("linux");
    let default_entry =
        libgaki::steam_client::select_launch_entry(&options, &game, prefer_windows, prefer_native)
            .map(|e| e.target);

    let steam = steam || !game.is_owned;
    let targets = libgaki::steam_client::launch_targets(
        &options,
        &game,
        libgaki::core::utils::host_steam_client_path().is_some(),
        steam,
        default_entry,
    );

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
    let mut game = find_game(&mut client, app_id).await?;

    // Otherwise the spawn fails on the missing directory with a raw OS error.
    if !game.is_installed {
        bail!(
            "{} is not installed — install it with `aurelia install {app_id}`",
            game.name
        );
    }

    // Family-Shared games are pinned to the owner's current build — a stale
    // install simply won't launch — so for them the pre-launch update is
    // *required*, not best-effort: any failure aborts the launch (running the old
    // build would just fail cryptically) and `--no-update` is ignored.
    let update_required = game.is_installed && game.is_family_shared;

    // Pull the latest build before launching. For owned games this is best-effort
    // (a failed check/download must not stop the user playing what's installed);
    // for Family-Shared games it is mandatory (see above).
    if game.is_installed && (!no_update || update_required) {
        if let Err(e) = client
            .check_for_updates(std::slice::from_mut(&mut game))
            .await
        {
            if update_required {
                bail!(
                    "could not verify the latest build for Family-Shared game {} ({e:#}); \
                     it must be up to date to launch",
                    game.name
                );
            }
            tracing::warn!("pre-launch update check failed ({e:#}); launching current version");
        }
        if game.update_available {
            if !json {
                let kind = if update_required {
                    "Required update"
                } else {
                    "Update available"
                };
                cli_println!("{kind} for {} — installing ...", game.name);
            }
            let state = Arc::new(RwLock::new(DownloadState::default()));
            // Registered like `aurelia update`: stoppable via `install stop`
            let update_result = match InstallGuard::register(app_id, Arc::clone(&state)) {
                Ok(_guard) => match client.update_game(app_id, state).await {
                    Ok(rx) => drive_progress(rx, json).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            if let Err(e) = update_result {
                if update_required {
                    bail!(
                        "required update for Family-Shared game {} failed ({e:#}); \
                         it can't launch until updated",
                        game.name
                    );
                }
                tracing::warn!("pre-launch update failed ({e:#}); launching current version");
            }
        }
    }

    // Warn-only headless DRM ownership preflight.
    if !steam && game.is_owned && !game.is_family_shared && !client.is_offline() {
        let probe = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.get_app_ticket(app_id),
        )
        .await;
        match probe {
            Ok(Ok(_)) => {
                tracing::info!(appid = app_id, "DRM preflight: ownership ticket issued");
            }
            Ok(Err(e)) => {
                let warning = format!(
                    "Steam issued no ownership ticket for {} ({e:#}). If the game uses \
                     Steam DRM it may not start standalone — try `aurelia play {app_id} --steam`.",
                    game.name
                );
                if json {
                    tracing::warn!(appid = app_id, "{warning}");
                } else {
                    cli_println!("⚠ {warning}");
                }
            }
            Err(_) => {
                tracing::warn!(appid = app_id, "DRM preflight timed out; launching anyway");
            }
        }
    }

    let launcher_config = load_launcher_config().await?;
    let game_cfg = launcher_config.game_configs.get(&app_id);
    let forced_proton = game_cfg
        .and_then(|c| c.forced_proton_version.clone())
        .filter(|_| !linux_target);
    let platform_preference = match platform {
        Some(PlatformArg::Linux) => Some("linux"),
        Some(PlatformArg::Windows) => Some("windows"),
        None => game_cfg.and_then(|c| c.platform_preference.as_deref()),
    };
    let prefers_windows = platform_preference == Some("windows");
    let prefers_linux = platform_preference == Some("linux");

    // Resolution order: explicit `--proton` flag → the game's stored version →
    // (when the game targets Windows) the global default. None means run natively.
    let proton_path = proton
        .or(forced_proton)
        .or_else(|| prefers_windows.then(|| launcher_config.proton_version.clone()));

    let user_configs = load_user_configs().await?;
    let user_config = user_configs.get(&app_id);

    if !json {
        cli_println!("Launching {} ...", game.name);
    }
    let announce_target: &(dyn Fn(Option<&str>) + Send + Sync) =
        &|proton: Option<&str>| match proton {
            Some(p) => cli_println!("Platform: Windows via Proton ({p})"),
            None => cli_println!("Platform: native Linux"),
        };
    client
        .play_game(
            &game,
            proton_path.as_deref(),
            user_config,
            script,
            no_script,
            steam,
            prefers_linux,
            (!json).then_some(&announce_target),
        )
        .await
        .with_context(|| format!("failed to launch {}", game.name))?;
    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "name": game.name,
            "status": "finished",
        }));
    } else {
        cli_println!("Finished playing {}.", game.name);
    }
    Ok(())
}

/// `aurelia running`: list the games Aurelia currently has running (stale records
/// whose process has exited are pruned).
pub(crate) fn cmd_running(json: bool) -> Result<()> {
    let running = libgaki::compat::running::list_active();
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

    let stopped = SteamClient::stop_game(app_id, force)
        .with_context(|| format!("failed to stop app {app_id}"))?;
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
