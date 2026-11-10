//! `install` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::{Context, Result};
use libgaki::core::error::{ErrorKind, TypedError, classify};
use libgaki::ops::install::{self, InstallRequest, Signalled};
use libgaki::steam_client::SteamClient;
use std::path::PathBuf;

pub(crate) async fn cmd_install(
    app_id: u32,
    platform: Option<PlatformArg>,
    restart_steam: bool,
    dry_run: bool,
    library: Option<String>,
) -> Result<()> {
    // `--dry-run` reports the size estimate and stops — it never touches Steam or
    // downloads anything.
    if dry_run {
        let mut client = authed_client().await?;
        return cmd_install_dry_run(&mut client, app_id, platform).await;
    }
    let request = InstallRequest {
        app_id,
        platform: platform.map(Into::into),
        restart_steam,
        library,
        on_disk: 0,
        auto_attempts: 0,
    };
    run_install(request).await
}

/// Install, keeping the backlog in step.
pub(crate) async fn run_install(request: InstallRequest) -> Result<()> {
    let app_id = request.app_id;
    let restart_steam = request.restart_steam;
    let mut client = authed_client().await?;
    let prepared = install::prepare(&mut client, request).await?;
    if prepared.auto_selected {
        cli_println!("Auto-selected platform: {:?}", prepared.platform);
    }
    let is_dlc = prepared.is_dlc;

    // A DLC edits its base game's appmanifest, which a running Steam overwrites
    // on exit: stop Steam first and restart it afterwards.
    let manage_steam = steam_guard_stop_optional(restart_steam && is_dlc)?;
    let settled = prepared
        .run(&client, drive_progress_until)
        .await
        .map_err(|e| resume_hint(app_id, e));
    // Always restore a Steam client that we stopped, including when the download
    // failed or was cancelled, without hiding the install error behind it.
    if let Err(restart_err) = steam_guard_restart(manage_steam) {
        if settled.is_err() {
            tracing::error!("failed restarting Steam after an install error: {restart_err:#}");
        } else {
            return Err(restart_err);
        }
    }
    settled?;

    // A newly installed DLC is invisible to an already-running Steam client until it
    // re-reads the appmanifest (which it does at startup).
    let steam_restart_required = is_dlc && !manage_steam && SteamClient::steam_is_running();
    if steam_restart_required {
        cli_eprintln!();
        cli_eprintln!(
            "Note: the DLC content is installed, but a running Steam client reads DLC state"
        );
        cli_eprintln!(
            "      only at startup. Restart Steam (or re-run with --restart-steam) for it to"
        );
        cli_eprintln!("      be recognized in-game.");
    }

    if !is_dlc {
        crate::commands::shaders::after_install(&client, app_id).await;
    }
    Ok(())
}

/// Point a paused install at `install resume`.
fn resume_hint(app_id: u32, e: anyhow::Error) -> anyhow::Error {
    let paused = libgaki::library::backlog::get(app_id)
        .ok()
        .flatten()
        .is_some_and(|entry| entry.state == libgaki::library::backlog::BacklogState::Paused);
    if paused && classify(&e).kind == ErrorKind::Cancelled {
        TypedError::new(
            ErrorKind::Cancelled,
            format!(
                "install of app {app_id} paused; continue with `gaki-cli install resume {app_id}`"
            ),
        )
        .into()
    } else {
        e
    }
}

/// Report the estimated download/disk size for installing `app_id` without
/// installing anything.
pub(crate) async fn cmd_install_dry_run(
    client: &mut SteamClient,
    app_id: u32,
    platform: Option<PlatformArg>,
) -> Result<()> {
    let install::Estimate {
        platform,
        size: est,
    } = install::estimate(client, app_id, platform.map(Into::into)).await?;

    let platform_str = format!("{platform:?}").to_lowercase();
    cli_println!("Install estimate for app {app_id} ({platform_str}):");
    cli_println!("  Download size: {}", human_bytes(est.download_size));
    cli_println!("  Disk size    : {}", human_bytes(est.disk_size));
    cli_println!("  Depots       : {}", est.depot_count);

    Ok(())
}

/// `gaki-cli install stop <app_id>`: signal a running install (tracked in this
/// process's registry) to abort. The download loops poll `abort_signal`.
pub(crate) async fn cmd_install_stop(app_id: u32) -> Result<()> {
    signal_install(app_id, false)
}

/// `gaki-cli install pause <app_id>`: stop, but stay `paused`.
pub(crate) async fn cmd_install_pause(app_id: u32) -> Result<()> {
    signal_install(app_id, true)
}

/// Stop or pause a running or queued install.
fn signal_install(app_id: u32, pause: bool) -> Result<()> {
    match install::signal(app_id, pause)? {
        Signalled::Running => {}
        Signalled::Elsewhere => {
            return Err(TypedError::new(
                ErrorKind::InvalidInput,
                format!(
                    "app {app_id} is downloading in another gaki process (another \
                     session daemon, or gaki-tui); stop it there"
                ),
            )
            .into());
        }
        Signalled::Relabelled(target) => {
            cli_println!(
                "App {app_id} is not downloading; marked {} in the backlog.",
                target.as_str()
            );

            return Ok(());
        }
        Signalled::NotFound => {
            cli_eprintln!(
                "no active install for app {app_id} (is the daemon running, and is an install in progress?)"
            );

            return Ok(());
        }
    }

    if pause {
        cli_println!("Pausing install of app {app_id} ...");
    } else {
        cli_println!("Stopping install of app {app_id} ...");
    }
    Ok(())
}

/// `gaki-cli install list`: running installs, then the backlog.
pub(crate) async fn cmd_install_list() -> Result<()> {
    let rows = install::list()?;

    if rows.is_empty() {
        cli_println!("No installs in progress or in the backlog.");
        return Ok(());
    }

    cli_println!(
        "{:>9}  {:<11}  {:>20}  {:>6}  NAME / STATUS",
        "APPID",
        "STATE",
        "PROGRESS",
        ""
    );
    for r in &rows {
        let progress = format!(
            "{} / {}",
            human_bytes(r.downloaded_bytes),
            human_bytes(r.total_bytes)
        );
        let label = match (r.name.is_empty(), r.status.is_empty()) {
            (false, true) => r.name.clone(),
            (true, _) => r.status.clone(),
            (false, false) if r.state == "running" => r.name.clone(),
            (false, false) => format!("{} ({})", r.name, r.status),
        };
        cli_println!(
            "{:>9}  {:<11}  {:>20}  {:>5.1}%  {}",
            r.app_id,
            r.state,
            progress,
            percent_of(r.downloaded_bytes, r.total_bytes),
            label
        );
    }
    Ok(())
}

/// `gaki-cli install resume <app_id>|--all`.
pub(crate) async fn cmd_install_resume(app_id: Option<u32>, all: bool) -> Result<()> {
    let ids = install::resume_targets(app_id, all).map_err(|e| {
        if classify(&e).kind == ErrorKind::NotFound {
            TypedError::new(
                ErrorKind::NotFound,
                format!("{e} (see `gaki-cli install list`)"),
            )
            .into()
        } else {
            e
        }
    })?;
    if ids.is_empty() {
        cli_println!("The install backlog is empty.");

        return Ok(());
    }
    resume_entries(&ids, false).await
}

/// Resume `ids` one after another.
///
/// `auto` counts attempts; manual resets them.
pub(crate) async fn resume_entries(ids: &[u32], auto: bool) -> Result<()> {
    let mut failures: Vec<(u32, anyhow::Error)> = Vec::new();
    for &app_id in ids {
        let Some((request, entry)) = install::resume_request(app_id, auto)? else {
            continue;
        };
        let name = if entry.name.is_empty() {
            format!("app {app_id}")
        } else {
            entry.name.clone()
        };
        if entry.state == libgaki::library::backlog::BacklogState::Queued {
            cli_println!("Installing {name} ...");
        } else {
            cli_println!("Resuming {name} ({}) ...", entry.state.as_str());
        }

        if let Err(e) = run_install(request).await {
            if ids.len() == 1 {
                return Err(e);
            }
            // Report it and keep going.
            cli_eprintln!("App {app_id}: {e:#}");
            failures.push((app_id, e));
        }
    }
    let Some((_, last)) = failures.last() else {
        return Ok(());
    };
    let kind = classify(last).kind;
    let summary = anyhow::anyhow!(TypedError::new(
        kind,
        format!("{} of {} resumes did not finish", failures.len(), ids.len()),
    ));
    Err(summary)
}

/// `gaki-cli install forget <app_id> [--delete-files]`.
pub(crate) async fn cmd_install_forget(app_id: u32, delete_files: bool) -> Result<()> {
    if install::is_running(app_id) {
        anyhow::bail!(
            "app {app_id} is downloading; stop it first with `gaki-cli install stop {app_id}`"
        );
    }
    let deleted = install::forget(app_id, delete_files)?;
    cli_println!("Removed app {app_id} from the install backlog.");
    if let Some(dir) = deleted {
        cli_println!("Deleted {}", dir.display());
    }

    Ok(())
}

pub(crate) async fn cmd_uninstall(app_id: u32, delete_prefix: bool) -> Result<()> {
    let client = authed_client().await?;
    install::uninstall(&client, app_id, delete_prefix).await?;
    cli_println!("Uninstalled app {app_id}.");

    Ok(())
}

pub(crate) async fn cmd_move(app_id: u32, library: PathBuf, restart_steam: bool) -> Result<()> {
    let client = authed_client().await?;

    // Steam rewrites appmanifests and libraryfolders.vdf on exit, so it must not be
    // running while we move things.
    let managed = steam_guard_stop(restart_steam)?;
    let outcome = install::move_to(&client, app_id, library.clone(), |rx| {
        drive_progress_until(rx)
    })
    .await;
    steam_guard_restart(managed)?;
    outcome?;

    cli_println!("Moved app {app_id} to {}.", library.display());

    Ok(())
}

/// Run a relink/import with Steam stopped.
async fn relocate_guarded<Fut>(
    library: PathBuf,
    restart_steam: bool,
    op: impl FnOnce(SteamClient, PathBuf) -> Fut,
) -> Result<PathBuf>
where
    Fut: Future<Output = Result<PathBuf>>,
{
    let client = authed_client().await?;
    let managed = steam_guard_stop(restart_steam)?;
    let result = op(client, library.clone()).await;
    steam_guard_restart(managed)?;
    result
}

pub(crate) async fn cmd_relink(app_id: u32, library: PathBuf, restart_steam: bool) -> Result<()> {
    relocate_guarded(library.clone(), restart_steam, |client, lib| async move {
        install::relink(&client, app_id, lib).await
    })
    .await?;
    cli_println!("Relinked app {app_id} to {}.", library.display());

    Ok(())
}

pub(crate) async fn cmd_import(
    app_id: u32,
    library: PathBuf,
    platform: Option<PlatformArg>,
    restart_steam: bool,
) -> Result<()> {
    let platform = platform.map(Into::into);
    let path = relocate_guarded(library, restart_steam, move |client, lib| async move {
        install::import(&client, app_id, lib, platform).await
    })
    .await?;
    cli_println!("Imported app {app_id} from {}.", path.display());

    Ok(())
}

pub(crate) async fn cmd_available(app_id: u32) -> Result<()> {
    // Local files only: no session, so per-game polling costs no Steam logon.
    let install::Availability {
        available,
        install_path,
        pinned,
        shader_cache,
        ..
    } = install::availability(app_id).await?;
    cli_println!(
        "App {app_id}: {}{}",
        if available {
            "available"
        } else {
            "not available"
        },
        if pinned { " (pinned)" } else { "" }
    );
    if let Some(p) = install_path {
        cli_println!("  path: {p}");
    }
    if let Some(cache) = shader_cache {
        cli_println!(
            "  shader cache: {} ({})",
            cache.path.display(),
            libgaki::core::utils::human_bytes(cache.bytes)
        );
    }

    Ok(())
}

pub(crate) async fn cmd_verify(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    install::verify(&client, app_id, drive_progress_until).await?;
    Ok(())
}

pub(crate) async fn cmd_update(app_id: u32, force: bool) -> Result<()> {
    // A pinned (downgraded) game must not be silently upgraded. `--force` overrides.
    let (pinned, _) = libgaki::core::config::game_pin_state(app_id).await;
    if pinned && !force {
        anyhow::bail!(
            "app {app_id} is pinned — run `gaki-cli unpin {app_id}` first (or pass --force)"
        );
    }

    let client = authed_client().await?;
    install::update(&client, app_id, force, drive_progress_until).await?;
    crate::commands::shaders::after_install(&client, app_id).await;
    Ok(())
}

/// `gaki-cli update` with no app id: list every installed game whose active branch
/// has a newer build available than what's on disk.
pub(crate) async fn cmd_check_updates() -> Result<()> {
    let mut client = authed_client().await?;
    let check = install::check_updates(&mut client).await?;
    if check.installed == 0 {
        cli_println!("No installed games found.");

        return Ok(());
    }
    let install::UpdateCheck {
        updates, pinned, ..
    } = check;

    if updates.is_empty() {
        cli_println!("All installed games are up to date.");
    } else {
        cli_println!("{:>9}  NAME", "APPID");
        for g in &updates {
            let branch = branch_suffix(g);
            cli_println!("{:>9}  {}{}", g.app_id, g.name, branch);
        }
        cli_println!(
            "\n{} game(s) need an update. Run `gaki-cli update <app_id>` to install one.",
            updates.len()
        );
    }

    if !pinned.is_empty() {
        cli_println!("\nPinned (held at a fixed version; `gaki-cli unpin <app_id>` to release):");
        for g in &pinned {
            cli_println!("{:>9}  {}", g.app_id, g.name);
        }
    }
    Ok(())
}

/// `gaki-cli manifests <app_id>`: list each depot's current manifest id per branch
/// (version discovery). Steam only exposes *current* ids; the printed SteamDB
/// links are where historical/older ids can be found for a `downgrade`.
#[cfg(feature = "versioning")]
pub(crate) async fn cmd_manifests(app_id: u32, depot: Option<u32>) -> Result<()> {
    let client = authed_client().await?;
    let manifests = install::manifests(&client, app_id, depot).await?;

    if manifests.is_empty() {
        cli_println!(
            "No depot manifests found for app {app_id}{}.",
            depot.map(|d| format!(" (depot {d})")).unwrap_or_default()
        );
        return Ok(());
    }

    cli_println!(
        "{:>9}  {:<14}  {:>20}  {:>12}  NAME",
        "DEPOT",
        "BRANCH",
        "MANIFEST_ID",
        "SIZE"
    );
    for m in &manifests {
        cli_println!(
            "{:>9}  {:<14}  {:>20}  {:>12}  {}",
            m.depot_id,
            m.branch,
            m.manifest_id,
            human_bytes(m.size),
            m.depot_name.as_deref().unwrap_or("")
        );
    }

    // Historical ids aren't in Steam's data — point at each depot's SteamDB page.
    let mut depot_ids: Vec<u32> = manifests.iter().map(|m| m.depot_id).collect();
    depot_ids.sort_unstable();
    depot_ids.dedup();
    cli_println!("\nOnly current manifest ids are shown (Steam does not expose older ones).");
    cli_println!("Find historical manifest ids for a downgrade on SteamDB:");
    for d in depot_ids {
        cli_println!("  {}", libgaki::core::steam_urls::steamdb_depot_url(d));
    }
    Ok(())
}

/// `gaki-cli downgrade`: install specific (usually older) depot manifests and pin
/// them, streaming progress via the same path as `install`.
#[cfg(feature = "versioning")]
pub(crate) async fn cmd_downgrade(args: DowngradeArgs) -> Result<()> {
    let overrides = install::parse_manifest_overrides(&args.depots, &args.manifests)?;

    if args.branch_password.is_some() {
        cli_eprintln!(
            "Note: --branch-password is recorded for reference only; the manifest ids you supply are downloaded directly."
        );
    }

    let mut client = authed_client().await?;

    cli_println!(
        "Downgrading app {} — pinning {} depot(s):",
        args.app_id,
        overrides.len()
    );
    let mut pairs: Vec<(&u32, &u64)> = overrides.iter().collect();
    pairs.sort();
    for (d, m) in pairs {
        cli_println!("  depot {d} -> manifest {m}");
    }

    let pinned = !args.no_pin;
    let request = install::Downgrade {
        app_id: args.app_id,
        manifests: overrides.clone(),
        library: args.library.clone(),
        branch: args.branch.clone(),
        verify: args.verify,
        pin: pinned,
    };
    // The second drive is the verify pass.
    let mut passes = 0;
    install::downgrade(&mut client, &request, |rx| {
        passes += 1;
        if passes == 2 {
            cli_println!("Verifying ...");
        }
        drive_progress_until(rx)
    })
    .await?;

    cli_println!(
        "Downgraded app {}{}.",
        args.app_id,
        if pinned { " and pinned it" } else { "" }
    );
    if pinned {
        cli_println!("gaki's `update` / `check-updates` will now hold it here. Note: launching");
        cli_println!("through the OFFICIAL Steam client can still re-queue an update (the pin is");
        cli_println!("authoritative only for gaki's own commands).");
    }

    Ok(())
}

/// `gaki-cli pin <app_id>`: lock gaki's update commands for a game, recording
/// its currently-installed manifests.
#[cfg(feature = "versioning")]
pub(crate) async fn cmd_pin(app_id: u32) -> Result<()> {
    let manifests = install::pin(app_id).await?;

    cli_println!(
        "Pinned app {app_id} to its installed manifests ({} depot(s)). gaki won't update it until `gaki-cli unpin {app_id}`.",
        manifests.len()
    );

    Ok(())
}

/// `gaki-cli unpin <app_id>`: release a game's version pin.
#[cfg(feature = "versioning")]
pub(crate) async fn cmd_unpin(app_id: u32) -> Result<()> {
    let was_pinned = install::unpin(app_id).await?;

    if was_pinned {
        cli_println!("Unpinned app {app_id}.");
    } else {
        cli_println!("App {app_id} was not pinned.");
    }
    Ok(())
}

/// Report games installed in more than one Steam library.
///
/// Deliberately read-only: the redundant copy is real game data on a real disk,
/// and which one to keep can depend on things gaki can't see (a drive about to
/// be unplugged, a slower disk). So this prints the removal commands rather than
/// running them.
pub(crate) async fn cmd_duplicates() -> Result<()> {
    let duplicates = libgaki::library::find_duplicate_installs()
        .await
        .context("could not scan Steam libraries for duplicate installs")?;

    if duplicates.is_empty() {
        cli_println!("No game is installed in more than one Steam library.");
        return Ok(());
    }

    cli_println!(
        "{} game(s) installed in more than one Steam library.\n\
         Each duplicate leaves two appmanifests disagreeing about what is installed,\n\
         which is why such a game can report an update on every check.\n",
        duplicates.len()
    );

    let mut reclaimable = 0u64;
    for d in &duplicates {
        let name = d
            .name
            .clone()
            .unwrap_or_else(|| format!("App {}", d.app_id));
        cli_println!("{name}  ({})", d.app_id);
        cli_println!(
            "  keep    {}  (updated {}, build {})",
            d.live.install_path.display(),
            format_unix_timestamp(d.live.last_updated),
            d.live.build_id
        );
        for s in &d.stale {
            let size = libgaki::library::relocate::dir_size(&s.install_path);
            reclaimable += size;
            cli_println!(
                "  remove  {}  (updated {}, build {}, {})",
                s.install_path.display(),
                format_unix_timestamp(s.last_updated),
                s.build_id,
                human_bytes(size)
            );
        }
        cli_println!("");
    }

    cli_println!(
        "Removing every copy above frees {}.\n",
        human_bytes(reclaimable)
    );
    cli_println!("Close Steam first — it rewrites appmanifests on exit — then run:\n");
    for d in &duplicates {
        for s in &d.stale {
            cli_println!(
                "  rm -rf {}",
                shell_quote(&s.install_path.to_string_lossy())
            );
            cli_println!(
                "  rm -f  {}",
                shell_quote(&s.manifest_path.to_string_lossy())
            );
        }
    }
    cli_println!(
        "\nDelete the appmanifest as well as the directory: on its own the manifest\n\
         still advertises the copy, and discovery would keep finding it."
    );
    Ok(())
}

/// Single-quote a path for a POSIX shell so a copy-pasted `rm` is safe with
/// spaces — which Steam install directories routinely contain.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
