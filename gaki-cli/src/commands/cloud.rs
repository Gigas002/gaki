//! `cloud` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::Result;
use std::path::PathBuf;

pub(crate) async fn cmd_cloud_sync(
    app_id: u32,
    up: bool,
    down: bool,
    path: Option<PathBuf>,
    resolve: Option<CloudResolve>,
) -> Result<()> {
    use libgaki::library::cloud_sync::{ConflictPolicy, SyncDirection};
    use libgaki::ops::cloud;

    let client = authed_client().await?;
    // No flag = full sync (down then up); `--down`/`--up` restrict the direction.
    let direction = if up {
        SyncDirection::Up
    } else if down {
        SyncDirection::Down
    } else {
        SyncDirection::Both
    };
    let direction_str = cloud::direction_name(direction);
    // Without `--resolve` divergent saves are only detected (both copies kept).
    let request = cloud::SyncRequest {
        app_id,
        direction,
        policy: resolve.map_or(ConflictPolicy::Detect, ConflictPolicy::from),
        remote_root: path,
    };
    let report = cloud::sync(&client, &request).await?;
    let cloud::SyncReport {
        wine_prefix,
        outcome,
        ..
    } = report;

    if outcome.has_conflicts() {
        cli_println!(
            "{} Cloud save(s) diverged from local — neither copy was changed:",
            outcome.conflicts.len()
        );
        for c in &outcome.conflicts {
            cli_println!(
                "  {}  (cloud {} bytes @ {}, local {} bytes @ {})",
                c.filename,
                c.cloud_size,
                c.cloud_timestamp,
                c.local_size,
                c.local_timestamp
            );
        }
        cli_println!(
            "\nResolve with: `gaki-cli cloud sync {app_id} --resolve cloud`  (use the Steam copy)\n            or `gaki-cli cloud sync {app_id} --resolve local`  (use the on-disk copy)"
        );
    } else {
        cli_println!(
            "Synced Cloud saves for app {app_id} ({direction_str}): {} down, {} up.",
            outcome.downloaded.len(),
            outcome.uploaded.len()
        );
    }

    // Name the destination. gaki can have several prefixes per game and syncing
    // into the wrong one looks exactly like success — the files are written, the
    // counts are right, and the game still starts with no saves.
    cli_println!("Save prefix: {}", wine_prefix.display());

    // A partial restore is the dangerous case: the game finds *some* files and
    // starts as though the save were intact, so this must never read as success.
    if outcome.has_failures() {
        cli_println!(
            "\nERROR: {} Cloud file(s) failed to transfer — this save set is INCOMPLETE:",
            outcome.failed.len()
        );
        for f in outcome.failed.iter().take(SKIP_SAMPLE) {
            cli_println!("  [{}] {}: {}", f.direction, f.filename, f.error);
        }
        if outcome.failed.len() > SKIP_SAMPLE {
            cli_println!("  ... and {} more", outcome.failed.len() - SKIP_SAMPLE);
        }
        cli_println!(
            "\nRe-run `gaki-cli cloud sync {app_id} --down` to retry just the missing files.\nDo not play until it completes — the game may overwrite a half-restored save."
        );
    }

    // A skipped file was never compared, so the counts above say nothing about it.
    // Report it loudly — the failure mode this replaces was a confident "0 down"
    // against a cloud full of saves.
    if outcome.has_skips() {
        let tokens = outcome.skipped_tokens();
        cli_println!(
            "\nWARNING: {} Cloud file(s) were skipped — gaki could not work out where they belong on disk.",
            outcome.skipped.len()
        );
        if tokens.is_empty() {
            for s in outcome.skipped.iter().take(SKIP_SAMPLE) {
                cli_println!("  {}: {}", s.filename, s.reason);
            }
        } else {
            cli_println!(
                "Unmapped save root token(s): {}",
                tokens
                    .iter()
                    .map(|t| format!("%{t}%"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for s in outcome.skipped.iter().take(SKIP_SAMPLE) {
                cli_println!("  {}", s.filename);
            }
        }
        if outcome.skipped.len() > SKIP_SAMPLE {
            cli_println!("  ... and {} more", outcome.skipped.len() - SKIP_SAMPLE);
        }
        if tokens.iter().any(|t| t.starts_with("Win")) {
            cli_println!(
                "\n%Win*% roots live inside the game's Proton prefix. Check that the game is\ninstalled and that its compatdata prefix exists (`gaki-cli info {app_id}`)."
            );
        } else {
            cli_println!(
                "\nThis token has no mapping in this build — please report it so it can be added."
            );
        }
        cli_println!("These saves are still safe in the Cloud; nothing was overwritten.");
    }
    Ok(())
}

/// How many skipped filenames to print before collapsing into a count — enough to
/// recognise which saves are affected without burying the explanation.
const SKIP_SAMPLE: usize = 5;

pub(crate) async fn cmd_cloud_list(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let files = libgaki::ops::cloud::files(&client, app_id).await?;

    if files.is_empty() {
        cli_println!("No Steam Cloud files for app {app_id}.");
        return Ok(());
    }
    cli_println!("{:>12}  {:<19}  NAME", "SIZE", "MODIFIED");
    let mut total = 0u64;
    for f in &files {
        total += f.size;
        cli_println!(
            "{:>12}  {:<19}  {}",
            human_bytes(f.size),
            format_unix_timestamp(f.timestamp),
            f.filename
        );
    }
    cli_println!("\n{} file(s), {}.", files.len(), human_bytes(total));
    Ok(())
}
