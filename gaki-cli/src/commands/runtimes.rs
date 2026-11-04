//! `proton` command handlers.

use crate::commands::common::*;

use anyhow::Result;

/// `proton list`: show the installable official Proton builds and what's installed.
pub(crate) async fn cmd_proton_list(installed_only: bool, json: bool) -> Result<()> {
    let listing = libgaki::ops::proton::listing().await?;
    let default = &listing.default;
    if installed_only {
        if json {
            print_json(&serde_json::json!({
                "default": listing.default,
                "installed": listing.installed,
            }));
            return Ok(());
        }
        if listing.installed.is_empty() {
            cli_println!("No Proton runtimes installed.");
            return Ok(());
        }
        cli_println!("Installed runtimes:");
        for i in &listing.installed {
            let star = if &i.name == default {
                "  * default"
            } else {
                ""
            };
            cli_println!("  {:<28} ({}){star}", i.name, i.location);
        }
        return Ok(());
    }

    if json {
        print_json(&listing);
        return Ok(());
    }

    cli_println!("{:<8}  {:<28}  {:>8}  STATUS", "SOURCE", "NAME", "APP ID");
    for pkg in &listing.available {
        let mut status = String::new();
        if listing.is_installed(&pkg.name) {
            status.push_str("installed");
        }
        if &pkg.name == default {
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
    for i in &listing.installed {
        if !listing
            .available
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&i.name))
        {
            let star = if &i.name == default { " *default" } else { "" };
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

/// `proton install`: install an official Proton build through Steam and make it
/// the global default.
pub(crate) async fn cmd_proton_install(version: String, json: bool) -> Result<()> {
    let pkg = libgaki::ops::proton::find(&version)?;
    if !json {
        cli_println!("Installing {} via Steam (app {}) ...", pkg.name, pkg.app_id);
    }
    let client = authed_client().await?;
    let rx = libgaki::ops::proton::start_install(&client, &pkg).await?;
    drive_progress(rx, json).await?;
    // The freshly installed runtime becomes the global default ("last downloaded").
    libgaki::ops::proton::set_default(&pkg.name).await?;

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
    let pkg = libgaki::ops::proton::find(&version)?;
    let client = authed_client().await?;
    let was_default = libgaki::ops::proton::uninstall(&client, &pkg).await?;

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

/// `proton default`: set the global default Proton version.
pub(crate) async fn cmd_proton_default(version: String, json: bool) -> Result<()> {
    let present = libgaki::ops::proton::set_default(&version).await?;
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
