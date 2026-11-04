//! `runtimes` command handlers.

use crate::commands::common::*;

use anyhow::{Context, Result};
use aurelia::core::config::load_launcher_config;
use aurelia::core::models::{DepotPlatform, DownloadState};
use aurelia::steam_client::InstallOptions;
use std::sync::{Arc, RwLock};

/// `proton list`: show installable runtimes and what's installed.
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
            cli_println!("No Proton/Wine runtimes installed.");
            return Ok(());
        }
        cli_println!("Installed runtimes:");
        for i in &installed {
            let star = if i.name == default { "  * default" } else { "" };
            cli_println!("  {:<28} ({}){star}", i.name, i.location);
        }
        return Ok(());
    }

    let available = aurelia::compat::proton::list_available()
        .await
        .unwrap_or_default();
    if json {
        print_json(&serde_json::json!({
            "default": default,
            "installed": installed,
            "available": available,
        }));
        return Ok(());
    }

    cli_println!("{:<10}  {:<28}  {:>10}  STATUS", "SOURCE", "NAME", "SIZE");
    for pkg in &available {
        let size = if pkg.size > 0 {
            human_bytes(pkg.size)
        } else {
            "-".to_string()
        };
        let mut status = String::new();
        if is_installed(&pkg.name) {
            status.push_str("installed");
        }
        if pkg.name == default {
            status.push_str(" *default");
        }
        cli_println!(
            "{:<10}  {:<28}  {:>10}  {}",
            pkg.label,
            pkg.name,
            size,
            status.trim()
        );
    }
    // Surface any installed runtime not present in the available list (e.g. an old GE
    // build no longer in recent releases) so the user still sees everything on disk.
    for i in &installed {
        if !available
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&i.name))
        {
            let star = if i.name == default { " *default" } else { "" };
            cli_println!(
                "{:<10}  {:<28}  {:>10}  installed{star}",
                "(local)",
                i.name,
                "-"
            );
        }
    }
    cli_println!("\nInstall with `aurelia proton install <NAME>`.");
    Ok(())
}

/// `proton install`: download/install a runtime and make it the global default.
pub(crate) async fn cmd_proton_install(version: String, json: bool) -> Result<()> {
    let pkg = aurelia::compat::proton::resolve_package(&version).await?;

    match &pkg.source {
        aurelia::compat::proton::ProtonSource::Valve { app_id } => {
            let app_id = *app_id;
            if !json {
                cli_println!("Installing {} via Steam (app {app_id}) ...", pkg.name);
            }
            let client = authed_client().await?;
            let state = Arc::new(RwLock::new(DownloadState::default()));
            let rx = client
                .install_game(
                    app_id,
                    DepotPlatform::Linux,
                    InstallOptions::default(),
                    state,
                )
                .await
                .with_context(|| format!("failed to start installing {}", pkg.name))?;
            drive_progress(rx, json).await?;
        }
        aurelia::compat::proton::ProtonSource::Github { .. } => {
            if !json {
                cli_println!("Downloading {} ({}) ...", pkg.name, pkg.label);
            }
            let mut on_progress = byte_progress_printer(json);
            let path =
                aurelia::compat::proton::install_github_package(&pkg, &mut on_progress).await?;
            if !json {
                cli_println!("\n  Extracted to {}", path.display());
            }
        }
    }

    // The freshly installed runtime becomes the global default ("last downloaded").
    let mut cfg = load_launcher_config().await?;
    cfg.proton_version = pkg.name.clone();
    cfg.save()
        .await
        .context("failed saving the default Proton version")?;

    if json {
        print_json(&serde_json::json!({
            "name": pkg.name,
            "status": "installed",
            "default": true,
        }));
    } else {
        cli_println!("Installed {} and set it as the global default.", pkg.name);
    }
    Ok(())
}

/// `proton uninstall`: delete an installed custom (GE) runtime.
pub(crate) async fn cmd_proton_uninstall(version: String, json: bool) -> Result<()> {
    aurelia::compat::proton::remove(&version)
        .with_context(|| format!("failed to uninstall {version}"))?;

    // If it was the global default, the default now points at something gone; warn.
    let cfg = load_launcher_config().await?;
    let was_default = cfg.proton_version.eq_ignore_ascii_case(&version);

    if json {
        print_json(&serde_json::json!({
            "name": version,
            "status": "uninstalled",
            "was_default": was_default,
        }));
    } else {
        cli_println!("Uninstalled {version}.");
        if was_default {
            cli_eprintln!(
                "Note: {version} was the global default — set a new one with \
                 `aurelia proton default <NAME>`."
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
