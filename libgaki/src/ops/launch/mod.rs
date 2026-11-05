//! Launching games: launch targets, the pre-launch update and DRM check,
//! Proton selection, and stopping running games.

use crate::compat::running::RunningGame;
use crate::config::{Platform, SteamIntegration};
use crate::core::config::load_launcher_config;
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::{DepotPlatform, DownloadProgress, LibraryGame};
use crate::ops::install::InstallGuard;
use crate::settings::{LaunchSettings, Overrides};
use crate::steam_client::{LaunchTargetChoice, SteamClient};
use anyhow::{Context, Result};
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

/// How long the DRM preflight waits for an ownership ticket.
const DRM_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(10);

fn require_installed(game: &LibraryGame) -> Result<()> {
    if game.is_installed {
        return Ok(());
    }
    Err(TypedError::new(
        ErrorKind::InvalidInput,
        format!("{} is not installed", game.name),
    )
    .into())
}

/// An installed game and the ways it can be launched.
#[derive(Debug, Clone)]
pub struct Targets {
    pub game: LibraryGame,
    pub targets: Vec<LaunchTargetChoice>,
}

/// Every build/Steam combination an installed game offers, marking the one
/// [`play`] would pick. Games the account doesn't own always need Steam.
pub async fn targets(client: &mut SteamClient, app_id: u32, steam: bool) -> Result<Targets> {
    let game = crate::ops::library::find_game(client, app_id).await?;
    require_installed(&game)?;
    let options = client
        .get_product_info(app_id)
        .await
        .with_context(|| format!("failed to load launch options for app {app_id}"))?;

    let settings = crate::settings::resolve_user(app_id, &Overrides::default())?;
    let steam = steam || settings.steam == SteamIntegration::Host;
    let default_entry = crate::steam_client::select_launch_entry(
        &options,
        &game,
        settings.platform == Some(Platform::Windows),
        settings.platform == Some(Platform::Linux),
    )
    .map(|e| e.target);

    let targets = crate::steam_client::launch_targets(
        &options,
        &game,
        crate::core::utils::host_steam_client_path().is_some(),
        steam || !game.is_owned,
        default_entry,
    );
    Ok(Targets { game, targets })
}

/// The Proton a launch runs under: the request's, then for a Windows launch
/// the profile's build or `default`. `None` lets the installed build decide.
fn choose_proton(
    request: Option<&str>,
    settings: &LaunchSettings,
    default: &str,
) -> Option<String> {
    request.map(str::to_owned).or_else(|| {
        (settings.platform == Some(Platform::Windows)).then(|| {
            settings
                .proton
                .as_ref()
                .and_then(|p| p.build.as_runner())
                .unwrap_or_else(|| default.to_owned())
        })
    })
}

/// How to launch a game.
#[derive(Debug, Clone, Default)]
pub struct PlayRequest {
    /// Proton to use, overriding the game's and the global setting.
    pub proton: Option<String>,
    /// Run with Steam integration (the host Steam client), overriding the
    /// game's and profile's `steam` setting.
    pub steam: bool,
    /// Build to run; `None` uses the game's preference. Linux runs the native
    /// build and can't be combined with `proton`.
    pub platform: Option<DepotPlatform>,
    /// Skip the pre-launch update (ignored for Family-Shared games, which
    /// can't launch out of date).
    pub no_update: bool,
}

/// Progress of [`play`], for the caller to report.
#[derive(Debug)]
pub enum LaunchEvent<'a> {
    /// An update is being installed before launch.
    Updating { required: bool },
    /// Steam issued no ownership ticket; a DRM-protected game may not start
    /// without Steam integration. Launching anyway.
    DrmUnverified { error: &'a anyhow::Error },
    /// About to start the game.
    Launching,
    /// The build was picked: under this Proton, or native when `None`.
    Target { proton: Option<&'a str> },
}

/// Launch an installed game (found with [`crate::ops::library::find_game`])
/// and wait for it to exit.
///
/// Before launching, the game is updated: best-effort for owned games, but
/// required for Family-Shared ones, which only run on the owner's current
/// build. `drive` consumes the update's progress.
pub async fn play<Fut: Future<Output = Result<bool>>>(
    client: &mut SteamClient,
    mut game: LibraryGame,
    request: &PlayRequest,
    on_event: &(dyn Fn(LaunchEvent<'_>) + Send + Sync),
    drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    let app_id = game.app_id;
    let linux_target = request.platform == Some(DepotPlatform::Linux);
    if linux_target && request.proton.is_some() {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            "a native Linux launch can't use Proton",
        )
        .into());
    }
    require_installed(&game)?;

    let launcher_config = load_launcher_config().await?;
    let overrides = Overrides {
        proton: request.proton.clone(),
        platform: request.platform.map(Platform::from),
        steam: request.steam.then_some(SteamIntegration::Host),
        ..Default::default()
    };
    // Before the update, so a config mistake fails fast.
    let settings = crate::settings::resolve_user(app_id, &overrides)?;

    let update_required = game.is_family_shared;
    if !request.no_update || update_required {
        pre_launch_update(client, &mut game, update_required, on_event, drive).await?;
    }

    // Warn-only: a DRM-free game runs fine without a ticket.
    if settings.steam != SteamIntegration::Host
        && game.is_owned
        && !game.is_family_shared
        && !client.is_offline()
    {
        match tokio::time::timeout(DRM_PREFLIGHT_TIMEOUT, client.get_app_ticket(app_id)).await {
            Ok(Ok(_)) => tracing::info!(appid = app_id, "DRM preflight: ownership ticket issued"),
            Ok(Err(error)) => on_event(LaunchEvent::DrmUnverified { error: &error }),
            Err(_) => tracing::warn!(appid = app_id, "DRM preflight timed out; launching anyway"),
        }
    }

    let proton_path = choose_proton(
        request.proton.as_deref(),
        &settings,
        &launcher_config.proton_version,
    );

    on_event(LaunchEvent::Launching);
    let on_target = |proton: Option<&str>| on_event(LaunchEvent::Target { proton });
    client
        .play_game(
            &game,
            proton_path.as_deref(),
            &settings,
            settings.steam == SteamIntegration::Host,
            settings.platform == Some(Platform::Linux),
            Some(&on_target),
        )
        .await
        .with_context(|| format!("failed to launch {}", game.name))?;
    Ok(())
}

/// Check for and install an update. Failures are logged, unless `required`.
async fn pre_launch_update<Fut: Future<Output = Result<bool>>>(
    client: &SteamClient,
    game: &mut LibraryGame,
    required: bool,
    on_event: &(dyn Fn(LaunchEvent<'_>) + Send + Sync),
    drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    if let Err(e) = client.check_for_updates(std::slice::from_mut(game)).await {
        if required {
            anyhow::bail!(
                "could not verify the latest build for Family-Shared game {} ({e:#}); it must \
                 be up to date to launch",
                game.name
            );
        }
        tracing::warn!("pre-launch update check failed ({e:#}); launching current version");
    }
    if !game.update_available {
        return Ok(());
    }
    on_event(LaunchEvent::Updating { required });
    let result = async {
        // Registered like any update, so it can be stopped.
        let (_guard, state) = InstallGuard::begin(game.app_id)?;
        let rx = client.update_game(game.app_id, state).await?;
        drive(rx).await
    }
    .await;
    if let Err(e) = result {
        if required {
            anyhow::bail!(
                "required update for Family-Shared game {} failed ({e:#}); it can't launch \
                 until updated",
                game.name
            );
        }
        tracing::warn!("pre-launch update failed ({e:#}); launching current version");
    }
    Ok(())
}

/// Games launched by this tool that are still running.
pub fn running() -> Vec<RunningGame> {
    crate::compat::running::list_active()
}

/// Stop a running game; `force` kills it outright.
pub fn stop(app_id: u32, force: bool) -> Result<RunningGame> {
    SteamClient::stop_game(app_id, force).with_context(|| format!("failed to stop app {app_id}"))
}

#[cfg(test)]
mod tests;
