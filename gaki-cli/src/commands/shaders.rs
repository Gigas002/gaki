//! `shaders`: Steam's pre-built shaders for a game, and the run after an
//! install or update.

use anyhow::Result;
use libgaki::ops::shaders::{self, Options, Replay};
use libgaki::steam_client::SteamClient;

use crate::commands::common::*;

/// `gaki-cli shaders <app_id> [--download-only]`: fetch the buckets, then
/// compile the pipelines even if they were compiled before.
pub(crate) async fn cmd_shaders(app_id: u32, download_only: bool, json: bool) -> Result<()> {
    let client = authed_client().await?;
    let options = Options {
        replay: !download_only,
        force_replay: true,
    };
    if !json && !download_only {
        cli_println!("Fetching and compiling shaders for app {app_id} ...");
    }
    let report = shaders::precache(&client, app_id, options).await?;
    if json {
        print_json(&report);
    } else {
        cli_println!("{}", report.summary());
        if let Some(hint) = replay_hint(&report.replay) {
            cli_println!("{hint}");
        }
    }
    Ok(())
}

/// Pre-cache after `app_id` installed or updated, when that's wanted. Prints
/// a line (not in JSON mode) and returns the report as JSON for the
/// caller's result line; a failure is a warning, never the install's error.
pub(crate) async fn after_install(
    client: &SteamClient,
    app_id: u32,
    json: bool,
) -> Option<serde_json::Value> {
    if !shaders::wanted_after_install(app_id) {
        return None;
    }
    if !json {
        cli_println!("Pre-caching shaders ...");
    }
    match shaders::precache(client, app_id, Options::default()).await {
        Ok(report) => {
            if !json {
                cli_println!("{}", report.summary());
            }
            serde_json::to_value(&report).ok()
        }
        Err(e) => {
            if !json {
                cli_eprintln!("Warning: shader pre-caching failed: {e:#}");
            }
            Some(serde_json::json!({ "error": format!("{e:#}") }))
        }
    }
}

fn replay_hint(replay: &Replay) -> Option<&'static str> {
    matches!(replay, Replay::NoReplayer).then_some(
        "Install Steam or fossilize (fossilize_replay on PATH) to compile them ahead of the \
         first launch; the drivers compile them while the game runs instead.",
    )
}
