//! `library` command handlers.

use libgaki::core::steam_urls;

use crate::commands::common::*;

use anyhow::{Context, Result};
use libgaki::ops::library::{self, ListOptions};

pub(crate) async fn cmd_list(
    installed: bool,
    search: Option<String>,
    collection: Option<String>,
    online: bool,
    check_updates: bool,
    json: bool,
) -> Result<()> {
    let mut client = restored_client().await?;
    let options = ListOptions {
        installed,
        search,
        collection,
        online,
        check_updates,
    };
    let listing = library::list(&mut client, &options).await?;
    let games = &listing.games;
    let game_collections = |app_id: u32| listing.collections_of(app_id).to_vec();

    if json {
        // Bake the conventional artwork/store URLs
        let enriched: Vec<serde_json::Value> = games
            .iter()
            .map(|g| {
                let mut v = serde_json::to_value(g).unwrap_or_default();
                if let Some(obj) = v.as_object_mut() {
                    obj.insert(
                        "assets".into(),
                        serde_json::json!({
                            "header": steam_urls::header_url(g.app_id),
                            "capsule": steam_urls::capsule_url(g.app_id),
                            "hero": steam_urls::hero_url(g.app_id),
                            "logo": steam_urls::logo_url(g.app_id),
                        }),
                    );
                    obj.insert(
                        "store_url".into(),
                        serde_json::json!(steam_urls::store_url(g.app_id)),
                    );
                    obj.insert(
                        "collections".into(),
                        serde_json::json!(game_collections(g.app_id)),
                    );
                }
                v
            })
            .collect();
        print_json(&enriched);
        return Ok(());
    }

    if games.is_empty() {
        cli_println!("No games match.");
        return Ok(());
    }

    if online {
        cli_println!(
            "{:>9}  {:<10}  {:<13}  {:<7}  {:<20}  NAME",
            "APPID",
            "STATUS",
            "LICENSE",
            "ONLINE",
            "COLLECTIONS"
        );
    } else {
        cli_println!(
            "{:>9}  {:<10}  {:<13}  {:<20}  NAME",
            "APPID",
            "STATUS",
            "LICENSE",
            "COLLECTIONS"
        );
    }
    for g in games {
        let status = if g.is_installed {
            if g.update_available {
                "update"
            } else {
                "installed"
            }
        } else {
            "-"
        };
        let license = if g.is_owned {
            "owned"
        } else if g.is_family_shared {
            "family-shared"
        } else {
            "unlicensed"
        };
        let branch = branch_suffix(g);
        let cols = game_collections(g.app_id);
        let cols = if cols.is_empty() {
            "-".to_string()
        } else {
            cols.join(", ")
        };
        if online {
            let online_col = match g.online_required {
                Some(true) => "yes",
                Some(false) => "no",
                None => "?",
            };
            cli_println!(
                "{:>9}  {:<10}  {:<13}  {:<7}  {:<20}  {}{}",
                g.app_id,
                status,
                license,
                online_col,
                cols,
                g.name,
                branch
            );
        } else {
            cli_println!(
                "{:>9}  {:<10}  {:<13}  {:<20}  {}{}",
                g.app_id,
                status,
                license,
                cols,
                g.name,
                branch
            );
        }
    }

    let shared = games.iter().filter(|g| g.is_family_shared).count();
    if shared > 0 {
        cli_println!(
            "\n{} game(s), {} via Family Sharing (not licensed to this account).",
            games.len(),
            shared
        );
    } else {
        cli_println!("\n{} game(s).", games.len());
    }
    Ok(())
}

pub(crate) async fn cmd_account(json: bool) -> Result<()> {
    let client = authed_client().await?;
    let data = library::account(&client).await;

    if json {
        let value = serde_json::json!({
            "steam_id": data.steam_id,
            "account_name": data.account_name,
            "persona_name": data.persona_name,
            "country": data.country,
            "email": data.email,
            "email_validated": data.email_validated,
            "authed_machines": data.authed_machines,
            "flags": data.flags,
            "vac_bans": data.vac_bans,
            "vac_banned_apps": data.vac_banned_apps,
        });
        print_json(&value);
        return Ok(());
    }

    cli_println!("Account : {}", data.account_name);
    if let Some(persona) = &data.persona_name {
        cli_println!("Persona : {persona}");
    }
    cli_println!("SteamID : {}", data.steam_id);
    cli_println!("Country : {}", data.country);
    cli_println!(
        "Email   : {} ({})",
        data.email,
        if data.email_validated {
            "validated"
        } else {
            "unvalidated"
        }
    );
    cli_println!("Devices : {}", data.authed_machines);
    cli_println!("VAC bans: {}", data.vac_bans);
    Ok(())
}

/// List the Steam library folders available to install into, with the free
/// space on each, one per line (or a JSON `{ "libraries": [{path, free_bytes}] }`
/// object with `--json`). Only roots that actually contain a `steamapps`
/// directory are reported.
pub(crate) async fn cmd_libraries(json: bool) -> Result<()> {
    let libraries: Vec<(String, Option<u64>)> = library::folders()
        .await
        .into_iter()
        .map(|f| (f.path.to_string_lossy().to_string(), f.free_bytes))
        .collect();

    if json {
        let entries: Vec<serde_json::Value> = libraries
            .iter()
            .map(|(path, free)| serde_json::json!({ "path": path, "free_bytes": free }))
            .collect();
        print_json(&serde_json::json!({ "libraries": entries }));
    } else if libraries.is_empty() {
        cli_println!("No Steam library folders found.");
    } else {
        for (path, free) in &libraries {
            match free {
                Some(bytes) => cli_println!("{path}  ({} free)", human_bytes(*bytes)),
                None => cli_println!("{path}"),
            }
        }
    }
    Ok(())
}

/// `gaki-cli library export [path] [--all]`.
pub(crate) async fn cmd_library_export(
    path: Option<std::path::PathBuf>,
    all: bool,
    json: bool,
) -> Result<()> {
    use libgaki::library::transfer;

    let mut client = restored_client().await?;
    let games = library::load_library(&mut client).await;
    let file = transfer::export(&games, all);
    let Some(path) = path else {
        if json {
            print_json(&serde_json::json!({ "games": file.games }));
        } else {
            cli_print!("{}", transfer::to_toml(&file)?);
        }
        return Ok(());
    };
    std::fs::write(&path, transfer::to_toml(&file)?)
        .with_context(|| format!("failed writing {}", path.display()))?;
    if json {
        print_json(&serde_json::json!({ "path": path, "games": file.games.len() }));
    } else {
        let n = file.games.len();
        let games = if n == 1 { "game" } else { "games" };
        cli_println!("Wrote {n} {games} to {}.", path.display());
    }
    Ok(())
}

/// `gaki-cli library import <path> [--library <dir>] [--dry-run]`.
#[cfg(feature = "install")]
pub(crate) async fn cmd_library_import(
    path: std::path::PathBuf,
    library_dir: Option<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    use libgaki::library::transfer::{self, LibraryEntry};

    let file = transfer::read(&path)?;
    // A dry run needs only the library, which the caches hold offline.
    let mut client = if dry_run {
        restored_client().await?
    } else {
        authed_client().await?
    };
    let games = library::load_library(&mut client).await;
    drop(client);
    let plan = transfer::plan(&file, &games);

    let ids = |entries: &[LibraryEntry]| entries.iter().map(|e| e.app_id).collect::<Vec<_>>();
    if dry_run {
        if json {
            print_json(&serde_json::json!({
                "install": ids(&plan.install),
                "installed": ids(&plan.installed),
                "not_owned": ids(&plan.not_owned),
            }));
        } else {
            for entry in &plan.install {
                match &entry.branch {
                    Some(branch) => {
                        cli_println!("install    {} (was on branch '{branch}')", label(entry))
                    }
                    None => cli_println!("install    {}", label(entry)),
                }
            }
            report_skipped(&plan.installed, &plan.not_owned);
        }
        return Ok(());
    }

    if json {
        for (entries, reason) in [
            (&plan.installed, "installed"),
            (&plan.not_owned, "not_owned"),
        ] {
            for entry in entries {
                print_result_line(serde_json::json!({
                    "app_id": entry.app_id,
                    "status": "skipped",
                    "reason": reason,
                }));
            }
        }
    } else {
        report_skipped(&plan.installed, &plan.not_owned);
    }
    if plan.install.is_empty() {
        if !json {
            cli_println!("Nothing to install.");
        }
        return Ok(());
    }
    let branched: Vec<&LibraryEntry> = plan.on_a_branch().collect();

    let queued = libgaki::ops::install::queue_import(&plan.install, library_dir).await?;
    if !json {
        let n = queued.len();
        let games = if n == 1 { "game" } else { "games" };
        cli_println!(
            "Queued {n} {games}; `gaki-cli install resume --all` continues if this stops."
        );
    }
    let result = crate::commands::install::resume_entries(&queued, false, json).await;

    if !json {
        // Installs take the default branch.
        for entry in branched {
            let branch = entry.branch.as_deref().unwrap_or_default();
            let id = entry.app_id;
            if cfg!(feature = "versioning") {
                cli_println!(
                    "{} was on branch '{branch}': switch with `gaki-cli set-branch {id} \
                     {branch}`, then `gaki-cli update {id}`.",
                    label(entry)
                );
            } else {
                cli_println!(
                    "{} was on branch '{branch}', which this build can't switch to.",
                    label(entry)
                );
            }
        }
    }
    result
}

#[cfg(feature = "install")]
fn label(entry: &libgaki::library::transfer::LibraryEntry) -> String {
    if entry.name.is_empty() {
        format!("app {}", entry.app_id)
    } else {
        format!("{} ({})", entry.name, entry.app_id)
    }
}

#[cfg(feature = "install")]
fn report_skipped(
    installed: &[libgaki::library::transfer::LibraryEntry],
    not_owned: &[libgaki::library::transfer::LibraryEntry],
) {
    for entry in installed {
        cli_println!("skip       {} (installed)", label(entry));
    }
    for entry in not_owned {
        cli_println!(
            "skip       {} (not in this account's library)",
            label(entry)
        );
    }
}
