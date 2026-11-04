//! Official Proton builds: listing, installing, uninstalling, and the global
//! default.

use crate::compat::proton::{
    InstalledProton, ProtonPackage, SYSTEM_COMPAT_TOOLS_DIR, find_official, list_installed,
    official_protons,
};
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::{DepotPlatform, DownloadProgress, DownloadState};
use crate::steam_client::{InstallOptions, SteamClient};
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Installed runtimes, installable official builds, and the global default.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProtonListing {
    pub default: String,
    pub installed: Vec<InstalledProton>,
    pub available: Vec<ProtonPackage>,
}

impl ProtonListing {
    /// Whether a runtime called `name` is on disk.
    pub fn is_installed(&self, name: &str) -> bool {
        self.installed
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(name))
    }
}

/// What's installed, what can be installed, and the current default.
pub async fn listing() -> Result<ProtonListing> {
    let config = crate::core::config::load_launcher_config().await?;
    Ok(ProtonListing {
        installed: list_installed(Path::new(&config.steam_library_path)),
        available: official_protons(),
        default: config.proton_version,
    })
}

/// Resolve `name` to an official Proton build, or explain why it can't be installed.
pub fn find(name: &str) -> Result<ProtonPackage> {
    find_official(name).ok_or_else(|| {
        TypedError::new(
            ErrorKind::NotFound,
            format!(
                "'{name}' is not an official Proton build. Custom builds such as GE or \
                 CachyOS are not downloaded; install them into {SYSTEM_COMPAT_TOOLS_DIR} or \
                 ~/.steam/root/compatibilitytools.d and they are picked up automatically."
            ),
        )
        .into()
    })
}

/// Start installing `package` through Steam; drive the returned progress
/// stream to completion, then call [`set_default`] to make it the default.
pub async fn start_install(
    client: &SteamClient,
    package: &ProtonPackage,
) -> Result<tokio::sync::mpsc::Receiver<DownloadProgress>> {
    let state = Arc::new(RwLock::new(DownloadState::default()));
    client
        .install_game(
            package.app_id,
            DepotPlatform::Linux,
            InstallOptions::default(),
            state,
        )
        .await
        .with_context(|| format!("failed to start installing {}", package.name))
}

/// Make `name` the global default Proton version. Returns whether a runtime
/// with that name is installed (the default is saved either way).
pub async fn set_default(name: &str) -> Result<bool> {
    let config = crate::ops::config::update(|c| {
        c.proton_version = name.to_string();
        Ok(())
    })
    .await
    .context("failed saving the default Proton version")?;
    Ok(list_installed(Path::new(&config.steam_library_path))
        .iter()
        .any(|i| i.name.eq_ignore_ascii_case(name)))
}

/// Uninstall `package` through Steam. Returns whether it was the global
/// default (which now points at something that's gone).
pub async fn uninstall(client: &SteamClient, package: &ProtonPackage) -> Result<bool> {
    client
        .uninstall_game(package.app_id, false)
        .await
        .with_context(|| format!("failed to uninstall {}", package.name))?;
    let config = crate::core::config::load_launcher_config().await?;
    Ok(find_official(&config.proton_version).is_some_and(|d| d.app_id == package.app_id))
}

#[cfg(test)]
mod tests;
