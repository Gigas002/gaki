//! `library` command handlers.

use libgaki::core::utils::format_playtime;

use crate::commands::common::*;

use anyhow::{Context, Result};
use libgaki::ops::library::{self, ListOptions};

pub(crate) async fn cmd_list(options: &ListOptions) -> Result<()> {
    let mut client = restored_client().await?;
    let listing = library::list(&mut client, options).await?;
    let games = &listing.games;
    let game_collections = |app_id: u32| listing.collections_of(app_id).to_vec();
    let online = options.online;

    if games.is_empty() {
        cli_println!("No games match.");
        return Ok(());
    }

    let online_head = if online {
        format!("{:<7}  ", "ONLINE")
    } else {
        String::new()
    };
    cli_println!(
        "{:>9}  {:<10}  {:<13}  {:>9}  {:<11}  {online_head}{:<20}  NAME",
        "APPID",
        "STATUS",
        "LICENSE",
        "PLAYTIME",
        "LAST PLAYED",
        "COLLECTIONS"
    );
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
        let playtime = match g.playtime_forever_minutes {
            Some(m) if m > 0 => format_playtime(m),
            _ => "-".to_string(),
        };
        let last_played = g.last_played.map_or_else(
            || "-".to_string(),
            |t| libgaki::steam_client::unix_to_ymd(t as i64),
        );
        let online_col = if online {
            let value = match g.online_required {
                Some(true) => "yes",
                Some(false) => "no",
                None => "?",
            };
            format!("{value:<7}  ")
        } else {
            String::new()
        };
        cli_println!(
            "{:>9}  {:<10}  {:<13}  {:>9}  {:<11}  {online_col}{:<20}  {}{}",
            g.app_id,
            status,
            license,
            playtime,
            last_played,
            cols,
            g.name,
            branch
        );
    }

    let shared = games.iter().filter(|g| g.is_family_shared).count();
    let total = listing.total_playtime_minutes();
    let total = format_playtime(u32::try_from(total).unwrap_or(u32::MAX));
    if shared > 0 {
        cli_println!(
            "\n{} game(s), {} via Family Sharing (not licensed to this account). Total playtime: {total}.",
            games.len(),
            shared
        );
    } else {
        cli_println!("\n{} game(s). Total playtime: {total}.", games.len());
    }
    Ok(())
}

pub(crate) async fn cmd_account() -> Result<()> {
    let client = authed_client().await?;
    let data = library::account(&client).await;

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
/// space on each, one per line: the configured `steam_library` first, then every
/// other root that contains a `steamapps` directory.
pub(crate) async fn cmd_libraries() -> Result<()> {
    let libraries: Vec<(String, Option<u64>)> = library::folders()
        .await
        .into_iter()
        .map(|f| (f.path.to_string_lossy().to_string(), f.free_bytes))
        .collect();

    if libraries.is_empty() {
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
pub(crate) async fn cmd_library_export(path: Option<std::path::PathBuf>, all: bool) -> Result<()> {
    use libgaki::library::transfer;

    let mut client = restored_client().await?;
    let games = library::load_library(&mut client).await;
    let file = transfer::export(&games, all);
    let Some(path) = path else {
        cli_print!("{}", transfer::to_toml(&file)?);

        return Ok(());
    };
    std::fs::write(&path, transfer::to_toml(&file)?)
        .with_context(|| format!("failed writing {}", path.display()))?;
    let n = file.games.len();
    let games = if n == 1 { "game" } else { "games" };
    cli_println!("Wrote {n} {games} to {}.", path.display());

    Ok(())
}

/// `gaki-cli library import <path> [--library <dir>] [--dry-run]`.
#[cfg(feature = "install")]
pub(crate) async fn cmd_library_import(
    path: std::path::PathBuf,
    library_dir: Option<String>,
    dry_run: bool,
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

    if dry_run {
        for entry in &plan.install {
            match &entry.branch {
                Some(branch) => {
                    cli_println!("install    {} (was on branch '{branch}')", label(entry))
                }
                None => cli_println!("install    {}", label(entry)),
            }
        }
        report_skipped(&plan.installed, &plan.not_owned);

        return Ok(());
    }

    report_skipped(&plan.installed, &plan.not_owned);

    if plan.install.is_empty() {
        cli_println!("Nothing to install.");

        return Ok(());
    }
    let branched: Vec<&LibraryEntry> = plan.on_a_branch().collect();

    let queued = libgaki::ops::install::queue_import(&plan.install, library_dir).await?;
    let n = queued.len();
    let games = if n == 1 { "game" } else { "games" };
    cli_println!("Queued {n} {games}; `gaki-cli install resume --all` continues if this stops.");

    let result = crate::commands::install::resume_entries(&queued, false).await;

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
