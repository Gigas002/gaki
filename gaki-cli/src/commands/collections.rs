//! `collections` command handlers.

use crate::commands::common::*;

use anyhow::Result;
use libgaki::ops::collections::{self, Kind};

/// `collections list`: show every collection (offline).
pub(crate) async fn cmd_collections_list() -> Result<()> {
    let (_, visible) = collections::list()?;

    if visible.is_empty() {
        cli_println!("No collections. Create one with `gaki-cli collections create <name>`,");
        cli_println!("or fetch your Steam collections with `gaki-cli collections pull`.");
        return Ok(());
    }

    cli_println!("{:<14}  {:<9}  {:>7}  NAME", "ID", "KIND", "GAMES");
    for c in &visible {
        let count = match &c.app_ids {
            Some(ids) => ids.len().to_string(),
            None => "-".to_string(),
        };
        cli_println!(
            "{:<14}  {:<9}  {:>7}  {}",
            c.id,
            c.kind.as_str(),
            count,
            c.name
        );
    }
    cli_println!("\n{} collection(s).", visible.len());
    Ok(())
}

/// `collections show <name>`: list a collection's member app ids (offline).
pub(crate) async fn cmd_collections_show(name: String) -> Result<()> {
    let c = collections::show(&name)?;
    let dynamic = c.kind == Kind::Dynamic;
    let members = c.app_ids.clone().unwrap_or_default();

    cli_println!("Collection : {}", c.name);
    cli_println!("Id         : {}", c.id);
    cli_println!("Kind       : {}", c.kind.as_str());
    if dynamic {
        cli_println!(
            "\nThis is a dynamic (filter-based) collection; Steam computes its members, \
             so they can't be listed offline."
        );
        return Ok(());
    }
    if members.is_empty() {
        cli_println!("\n(no games)");
    } else {
        cli_println!("\nGames ({}):", members.len());
        for app_id in members {
            cli_println!("  {app_id}");
        }
    }
    Ok(())
}

/// `collections create <name>`: make a new static collection (offline).
pub(crate) async fn cmd_collections_create(name: String) -> Result<()> {
    let id = collections::create(&name)?;
    cli_println!("Created collection '{name}' ({id}).");
    cli_println!("Add games with `gaki-cli collections add \"{name}\" <appid> ...`, then");
    cli_println!("`gaki-cli collections push` to upload it to your Steam account.");

    Ok(())
}

/// `collections delete <name>`: mark a collection for deletion (offline).
pub(crate) async fn cmd_collections_delete(name: String) -> Result<()> {
    collections::delete(&name)?;
    cli_println!("Marked collection '{name}' for deletion.");
    cli_println!("Run `gaki-cli collections push` to apply it to your Steam account.");

    Ok(())
}

/// `collections rename <name> <new_name>` (offline).
pub(crate) async fn cmd_collections_rename(name: String, new_name: String) -> Result<()> {
    collections::rename(&name, &new_name)?;
    cli_println!("Renamed '{name}' to '{new_name}'.");

    Ok(())
}

/// `collections add <name> <appid>...` (offline).
pub(crate) async fn cmd_collections_add(name: String, app_ids: Vec<u32>) -> Result<()> {
    collections::add(&name, &app_ids)?;
    cli_println!("Added {} game(s) to '{name}'.", app_ids.len());

    Ok(())
}

/// `collections remove <name> <appid>...` (offline).
pub(crate) async fn cmd_collections_remove(name: String, app_ids: Vec<u32>) -> Result<()> {
    collections::remove(&name, &app_ids)?;
    cli_println!("Removed {} game(s) from '{name}'.", app_ids.len());

    Ok(())
}

/// `collections pull`: download and merge Steam's collections into the local store.
pub(crate) async fn cmd_collections_pull() -> Result<()> {
    let client = authed_client().await?;
    let collections::Synced {
        namespace_version,
        count,
    } = collections::pull(&client).await?;
    cli_println!(
        "Pulled collections from Steam. {count} collection(s) locally (version {}).",
        namespace_version
    );

    Ok(())
}

/// `collections push`: upload local collections to Steam (mutates the account).
pub(crate) async fn cmd_collections_push(yes: bool) -> Result<()> {
    confirm_cloud_write("upload", collections::local_count()?, yes)?;
    let client = authed_client().await?;
    let collections::Synced {
        namespace_version,
        count,
    } = collections::push(&client).await?;
    cli_println!(
        "Pushed collections to Steam. Now at version {} ({count} collection(s)).",
        namespace_version
    );

    Ok(())
}

/// `collections sync`: pull then push (mutates the account).
pub(crate) async fn cmd_collections_sync(yes: bool) -> Result<()> {
    confirm_cloud_write("sync", collections::local_count()?, yes)?;
    let client = authed_client().await?;
    let collections::Synced {
        namespace_version,
        count,
    } = collections::sync(&client).await?;
    cli_println!(
        "Synced collections with Steam. Now at version {} ({count} collection(s)).",
        namespace_version
    );

    Ok(())
}
