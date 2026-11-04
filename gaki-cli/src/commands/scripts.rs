//! `scripts` command handlers.

use crate::commands::common::*;

use anyhow::Result;
use libgaki::launch::launch_script;

/// `scripts dir`: print the resolved launch-script directory.
pub(crate) async fn cmd_scripts_dir(json: bool) -> Result<()> {
    let dir = launch_script::script_dir();
    if json {
        print_json(&serde_json::json!({ "script_dir": dir }));
    } else {
        cli_println!("{}", dir.display());
    }
    Ok(())
}

/// `scripts list`: games with a launch script and their resolved paths.
pub(crate) async fn cmd_scripts_list(json: bool) -> Result<()> {
    let scripts = libgaki::ops::scripts::list().await?;
    if json {
        print_json(&serde_json::json!({ "scripts": scripts }));
        return Ok(());
    }
    if scripts.is_empty() {
        cli_println!("No launch scripts. Create one with `gaki-cli scripts new <app_id>`.");
        return Ok(());
    }
    cli_println!(
        "Launch scripts (dir: {}):",
        launch_script::script_dir().display()
    );
    for s in scripts {
        let name = s.name.as_deref().unwrap_or("(unknown)");
        match s.path {
            Some(p) => cli_println!("  {:<10} {name}\n             {}", s.app_id, p.display()),
            None => cli_println!("  {:<10} {name}  (unresolved)", s.app_id),
        }
    }
    Ok(())
}

/// `scripts new`: scaffold a launch script at `<script_dir>/<app_id>.sh`.
pub(crate) async fn cmd_scripts_new(app_id: u32, force: bool, json: bool) -> Result<()> {
    let path = libgaki::ops::scripts::create(app_id, force)
        .await
        .map_err(|e| {
            if libgaki::core::error::classify(&e).kind
                == libgaki::core::error::ErrorKind::InvalidInput
            {
                anyhow::anyhow!("{e} (use --force to overwrite)")
            } else {
                e
            }
        })?;
    if json {
        print_json(&serde_json::json!({ "app_id": app_id, "path": path, "status": "created" }));
    } else {
        cli_println!("Created launch script: {}", path.display());
        cli_println!("Edit it, then `gaki-cli play {app_id}` runs through it.");
    }
    Ok(())
}

/// `scripts show`: print a game's resolved launch-script path and contents.
pub(crate) async fn cmd_scripts_show(app_id: u32, json: bool) -> Result<()> {
    let Some(script) = libgaki::ops::scripts::show(app_id).await? else {
        if json {
            print_json(&serde_json::json!({ "app_id": app_id, "path": null, "exists": false }));
        } else {
            cli_println!("No launch script for app {app_id}.");
        }
        return Ok(());
    };
    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "path": script.path,
            "exists": script.contents.is_some(),
            "contents": script.contents,
        }));
    } else {
        cli_println!("Launch script for app {app_id}: {}", script.path.display());
        match script.contents {
            Some(c) => {
                cli_println!("");
                cli_print!("{c}");
            }
            None => cli_println!("(file does not exist on disk)"),
        }
    }
    Ok(())
}

/// `scripts remove`: delete a game's dir-based launch script.
pub(crate) async fn cmd_scripts_remove(app_id: u32, json: bool) -> Result<()> {
    let (path, removed) = libgaki::ops::scripts::remove(app_id)?;
    if json {
        print_json(&serde_json::json!({
            "app_id": app_id,
            "path": path,
            "status": if removed { "removed" } else { "not_found" },
        }));
    } else if removed {
        cli_println!("Removed launch script: {}", path.display());
    } else {
        cli_println!(
            "No dir-based launch script for app {app_id} at {}.",
            path.display()
        );
    }
    Ok(())
}
