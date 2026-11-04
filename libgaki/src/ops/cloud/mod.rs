//! Steam Cloud saves: listing and two-way sync.

use crate::library::cloud_sync::{
    CloudFileEntry, CloudPathResolver, ConflictPolicy, SyncDirection, SyncOutcome,
    default_cloud_root,
};
use crate::steam_client::SteamClient;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// A cloud sync to run.
#[derive(Debug, Clone)]
pub struct SyncRequest {
    pub app_id: u32,
    pub direction: SyncDirection,
    /// [`ConflictPolicy::Detect`] leaves both copies of a diverged save alone.
    pub policy: ConflictPolicy,
    /// Base for classic (token-less) files; defaults to `<appid>/remote` under
    /// the account's cloud directory.
    pub remote_root: Option<PathBuf>,
}

/// What a sync did, and where.
#[derive(Debug)]
pub struct SyncReport {
    pub remote_root: PathBuf,
    /// The Proton prefix `%Win*%` roots were resolved against.
    pub wine_prefix: PathBuf,
    pub outcome: SyncOutcome,
}

impl SyncReport {
    /// `failed`, `conflicts`, `incomplete` (files skipped) or `ok`.
    pub fn status(&self) -> &'static str {
        status_of(&self.outcome)
    }
}

fn status_of(outcome: &SyncOutcome) -> &'static str {
    if outcome.has_failures() {
        "failed"
    } else if outcome.has_conflicts() {
        "conflicts"
    } else if outcome.has_skips() {
        "incomplete"
    } else {
        "ok"
    }
}

/// Lower-case name of a direction.
pub fn direction_name(direction: SyncDirection) -> &'static str {
    match direction {
        SyncDirection::Up => "up",
        SyncDirection::Down => "down",
        SyncDirection::Both => "both",
    }
}

/// Sync an app's cloud saves.
///
/// Classic files live under the remote root; Auto-Cloud files resolve via
/// their `%RootToken%` prefix, which may need the install directory or the
/// game's Proton prefix. The app's UFS save rules let an upload find
/// brand-new local saves.
pub async fn sync(client: &SteamClient, request: &SyncRequest) -> Result<SyncReport> {
    let app_id = request.app_id;
    let cloud = client.cloud_client()?;
    let remote_root = match &request.remote_root {
        Some(p) => p.clone(),
        None => default_cloud_root(cloud.steam_id(), app_id)
            .context("could not resolve the local cloud save directory")?
            .join("remote"),
    };
    let (_, install_path) = client.is_game_available(app_id).await;
    let launcher_config = crate::core::config::LauncherConfig::load().await?;
    let wine_prefix = crate::core::utils::proton_compat_prefix(&launcher_config, app_id);
    let resolver = CloudPathResolver::new(remote_root.clone(), install_path.map(PathBuf::from))
        .with_wine_prefix(Some(wine_prefix.clone()));
    let specs = client
        .fetch_ufs_save_specs(app_id)
        .await
        .unwrap_or_default();
    let outcome = cloud
        .sync(app_id, &resolver, &specs, request.direction, request.policy)
        .await
        .with_context(|| format!("cloud sync failed for app {app_id}"))?;
    Ok(SyncReport {
        remote_root,
        wine_prefix,
        outcome,
    })
}

/// An app's cloud files, sorted by name.
pub async fn files(client: &SteamClient, app_id: u32) -> Result<Vec<CloudFileEntry>> {
    let mut files = client
        .cloud_client()?
        .get_file_list(app_id)
        .await
        .with_context(|| format!("failed listing Cloud files for app {app_id}"))?;
    files.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(files)
}

#[cfg(test)]
mod tests;
