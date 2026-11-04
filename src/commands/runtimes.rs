//! `runtimes` command handlers.

use crate::commands::common::*;

use anyhow::{Context, Result};
use aurelia::core::config::load_launcher_config;
use aurelia::core::models::{DepotPlatform, DownloadState};
use aurelia::steam_client::InstallOptions;
use std::sync::{Arc, RwLock};

/// `proton list`: show the installable official Proton builds and what's installed.
pub(crate) async fn cmd_proton_list(installed_only: bool, json: bool) -> Result<()> {
    let cfg = load_launcher_config().await?;
    let installed =
        aurelia::compat::proton::list_installed(std::path::Path::new(&cfg.steam_library_path));
    let default = cfg.proton_version;
    let is_installed = |name: &str| installed.iter().any(|i| i.name.eq_ignore_ascii_case(name));
    if installed_only {
        if json {
            print_json(&serde_json::json!({ "default": default, "installed": installed }));
            return Ok(());
        }
        if installed.is_empty() {
            cli_println!("No Proton runtimes installed.");
            return Ok(());
        }
        cli_println!("Installed runtimes:");
        for i in &installed {
            let star = if i.name == default { "  * default" } else { "" };
            cli_println!("  {:<28} ({}){star}", i.name, i.location);
        }
        return Ok(());
    }

    let available = aurelia::compat::proton::official_protons();
    if json {
        print_json(&serde_json::json!({
            "default": default,
            "installed": installed,
            "available": available,
        }));
        return Ok(());
    }

    cli_println!("{:<8}  {:<28}  {:>8}  STATUS", "SOURCE", "NAME", "APP ID");
    for pkg in &available {
        let mut status = String::new();
        if is_installed(&pkg.name) {
            status.push_str("installed");
        }
        if pkg.name == default {
            status.push_str(" *default");
        }
        cli_println!(
            "{:<8}  {:<28}  {:>8}  {}",
            "official",
            pkg.name,
            pkg.app_id,
            status.trim()
        );
    }
    // Runtimes found on disk that aren't in the official list (custom builds in
    // a compatibilitytools.d, or an older official release).
    for i in &installed {
        if !available
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&i.name))
        {
            let star = if i.name == default { " *default" } else { "" };
            cli_println!(
                "{:<8}  {:<28}  {:>8}  installed{star}",
                i.location,
                i.name,
                "-"
            );
        }
    }
    cli_println!("\nInstall with `aurelia proton install <NAME>`.");
    Ok(())
}

/// Resolve `name` to an official Proton build, or explain why it can't be installed.
fn official_or_bail(name: &str) -> Result<aurelia::compat::proton::ProtonPackage> {
    aurelia::compat::proton::find_official(name).ok_or_else(|| {
        anyhow::anyhow!(
            "'{name}' is not an official Proton build (see `aurelia proton list`). \
             Custom builds such as GE or CachyOS are not downloaded; install them into \
             {} or ~/.steam/root/compatibilitytools.d and they are picked up automatically.",
            aurelia::compat::proton::SYSTEM_COMPAT_TOOLS_DIR
        )
    })
}

/// `proton install`: install an official Proton build through Steam and make it
/// the global default.
pub(crate) async fn cmd_proton_install(version: String, json: bool) -> Result<()> {
    let pkg = official_or_bail(&version)?;
    if !json {
        cli_println!("Installing {} via Steam (app {}) ...", pkg.name, pkg.app_id);
    }
    let client = authed_client().await?;
    let state = Arc::new(RwLock::new(DownloadState::default()));
    let rx = client
        .install_game(
            pkg.app_id,
            DepotPlatform::Linux,
            InstallOptions::default(),
            state,
        )
        .await
        .with_context(|| format!("failed to start installing {}", pkg.name))?;
    drive_progress(rx, json).await?;

    // The freshly installed runtime becomes the global default ("last downloaded").
    let mut cfg = load_launcher_config().await?;
    cfg.proton_version = pkg.name.clone();
    cfg.save()
        .await
        .context("failed saving the default Proton version")?;

    if json {
        print_json(&serde_json::json!({
            "name": pkg.name,
            "app_id": pkg.app_id,
            "status": "installed",
            "default": true,
        }));
    } else {
        cli_println!("Installed {} and set it as the global default.", pkg.name);
    }
    Ok(())
}

/// `proton uninstall`: uninstall an official Proton build through Steam.
pub(crate) async fn cmd_proton_uninstall(version: String, json: bool) -> Result<()> {
    let pkg = official_or_bail(&version)?;
    let client = authed_client().await?;
    client
        .uninstall_game(pkg.app_id, false)
        .await
        .with_context(|| format!("failed to uninstall {}", pkg.name))?;

    // If it was the global default, the default now points at something gone; warn.
    let cfg = load_launcher_config().await?;
    let was_default = aurelia::compat::proton::find_official(&cfg.proton_version)
        .is_some_and(|d| d.app_id == pkg.app_id);

    if json {
        print_json(&serde_json::json!({
            "name": pkg.name,
            "app_id": pkg.app_id,
            "status": "uninstalled",
            "was_default": was_default,
        }));
    } else {
        cli_println!("Uninstalled {}.", pkg.name);
        if was_default {
            cli_eprintln!(
                "Note: {} was the global default — set a new one with \
                 `aurelia proton default <NAME>`.",
                pkg.name
            );
        }
    }
    Ok(())
}

/// `proton default`: set the global default Proton/Wine version.
pub(crate) async fn cmd_proton_default(version: String, json: bool) -> Result<()> {
    let mut cfg = load_launcher_config().await?;
    let installed =
        aurelia::compat::proton::list_installed(std::path::Path::new(&cfg.steam_library_path));
    let present = installed
        .iter()
        .any(|i| i.name.eq_ignore_ascii_case(&version));

    cfg.proton_version = version.clone();
    cfg.save()
        .await
        .context("failed saving the default Proton version")?;

    if json {
        print_json(&serde_json::json!({
            "default": version,
            "installed_present": present,
        }));
    } else {
        cli_println!("Global default Proton set to {version}.");
        if !present {
            cli_eprintln!(
                "Note: '{version}' is not installed — run `aurelia proton install {version}`."
            );
        }
    }
    Ok(())
}
