//! Cross-cutting command helpers shared across command modules.

use crate::daemon;

use anyhow::{Context, Result, bail};
use libgaki::core::error::{ErrorKind, TypedError, classify};
use libgaki::core::models::{DownloadProgress, DownloadProgressState, LibraryGame};
use libgaki::ops::session::RestoreFailure;
use libgaki::steam_client::SteamClient;
use std::io::Write;

// Library operations used across the command modules.
pub(crate) use libgaki::ops::install::InstallGuard;
pub(crate) use libgaki::ops::library::{find_game, load_library, merge_family_shared};
pub(crate) use libgaki::ops::progress::{RateTracker, percent_of};

/// Pretty-print JSON to stdout.
pub(crate) fn print_json<T: serde::Serialize + ?Sized>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => cli_println!("{s}"),
        Err(e) => {
            cli_eprintln!("failed serializing JSON output: {e}");
            cli_println!("{{}}");
        }
    }
}

/// Print a JSON value as a single compact line (for NDJSON streams).
pub(crate) fn print_json_line(value: &serde_json::Value) {
    match serde_json::to_string(value) {
        Ok(s) => cli_println!("{s}"),
        Err(_) => cli_println!("{{}}"),
    }
}

/// Version of the `--json` contract.
pub(crate) const JSON_SCHEMA: u32 = 1;

/// Print a `result` event, schema-stamped.
pub(crate) fn print_result_line(mut value: serde_json::Value) {
    value["event"] = "result".into();
    value["schema"] = JSON_SCHEMA.into();
    print_json_line(&value);
}

/// Emit an intermediate NDJSON *event* (progress ticks, login challenges) to
/// **stderr**, keeping stdout reserved for the single terminal result object so
/// a driver like Heroic can `JSON.parse` stdout without scanning for the last
/// value.
pub(crate) fn eprint_json_line(value: &serde_json::Value) {
    match serde_json::to_string(value) {
        Ok(s) => cli_eprintln!("{s}"),
        Err(_) => cli_eprintln!("{{}}"),
    }
}

/// Build a client and restore the stored session, if there is one.
///
/// Inside the daemon this returns a cheap client backed by the **single shared
/// connection** (no logon); only a standalone run actually re-authenticates here.
pub(crate) async fn restored_client() -> Result<SteamClient> {
    if daemon::in_daemon() {
        return Ok(daemon::shared_restored_client().await);
    }
    libgaki::ops::session::restore_client().await
}

/// Require an authenticated client, with a hint on how to log in.
pub(crate) async fn authed_client() -> Result<SteamClient> {
    let client = restored_client().await?;
    // The daemon's restore failure explains why better than "not logged in".
    let prior = daemon::last_restore_error()
        .await
        .map(|(kind, retry_after, reason)| RestoreFailure {
            kind,
            retry_after,
            reason,
        });
    libgaki::ops::session::require_authenticated(client, prior).map_err(|e| {
        if e.to_string() == "not logged in" {
            TypedError::new(
                ErrorKind::AuthRequired,
                "not logged in — run `aurelia login` first",
            )
            .into()
        } else {
            e
        }
    })
}

/// A web session for the web-surface commands (inventory, wallet, market
/// listings): minted off the CM session, or built from the stored browser web
/// token (`login --web-token`).
pub(crate) async fn web_access() -> Result<libgaki::web::web_session::WebSession> {
    let client = restored_client().await?;
    libgaki::ops::session::web_session(&client)
        .await
        .map_err(|e| {
            if classify(&e).kind == ErrorKind::AuthRequired {
                TypedError::new(
                    ErrorKind::AuthRequired,
                    "not logged in — run `aurelia login` first, or `aurelia login --web-token` \
                     for web-only (inventory/wallet/market) access",
                )
                .into()
            } else {
                e
            }
        })
}

/// User argument to SteamID64; the daemon answers names from its roster.
pub(crate) async fn resolve_user_ident(client: &SteamClient, input: &str) -> Result<u64> {
    let roster = if daemon::in_daemon() {
        Some(daemon::shared_roster().await)
    } else {
        None
    };
    libgaki::ops::session::resolve_user_ident(client, input, roster).await
}

/// Confirm action; honors `--yes`/`--json`.
pub(crate) fn confirm_write(action: &str, prompt: &str, yes: bool, json: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if json {
        bail!("refusing to {action} without confirmation — pass `--yes` in --json mode");
    }
    let answer = prompt_line(prompt)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        bail!("aborted");
    }
}

/// Cloud-collections wrapper over [`confirm_write`].
pub(crate) fn confirm_cloud_write(action: &str, count: usize, yes: bool, json: bool) -> Result<()> {
    confirm_write(
        action,
        &format!("About to {action} {count} collection(s) to your Steam account. Continue? [y/N] "),
        yes,
        json,
    )
}

/// Human label for a raw EPersonaState value.
pub(crate) fn persona_state_label(state: Option<u32>) -> &'static str {
    match state {
        Some(0) | None => "offline",
        Some(1) => "online",
        Some(2) => "busy",
        Some(3) => "away",
        Some(4) => "snooze",
        Some(5) => "looking to trade",
        Some(6) => "looking to play",
        Some(_) => "online",
    }
}

pub(crate) fn yesno(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

pub(crate) use libgaki::core::utils::human_bytes;

/// Refuse an edit while Steam runs, or stop it with `--restart-steam`.
/// Returns whether Steam was stopped (pass it to [`steam_guard_restart`]).
pub(crate) fn steam_guard_stop(restart_steam: bool, json: bool) -> Result<bool> {
    libgaki::ops::steam::stop_for_edit(restart_steam, || {
        if !json {
            cli_println!("Stopping Steam ...");
        }
    })
    .map_err(|e| {
        if classify(&e).kind == ErrorKind::InvalidInput {
            anyhow::anyhow!(
                "Steam is running. Close it first, or re-run with --restart-steam to have \
                 Aurelia stop and restart it around the change."
            )
        } else {
            e
        }
    })
}

/// Stop Steam if wanted and running.
pub(crate) fn steam_guard_stop_optional(want: bool, json: bool) -> Result<bool> {
    libgaki::ops::steam::stop_if_running(want, || {
        if !json {
            cli_println!("Stopping Steam ...");
        }
    })
}

/// Restart Steam if [`steam_guard_stop`] stopped it.
pub(crate) fn steam_guard_restart(managed: bool, json: bool) -> Result<()> {
    libgaki::ops::steam::restart_after_edit(managed, || {
        if !json {
            cli_println!("Starting Steam ...");
        }
    })
}

/// Non-default branch suffix, else empty.
pub(crate) fn branch_suffix(game: &LibraryGame) -> String {
    if game.active_branch != "public" {
        format!(" [{}]", game.active_branch)
    } else {
        String::new()
    }
}

/// Print the final result of a streaming operation (install/verify/update).
pub(crate) fn report_operation(app_id: u32, status: &str, json: bool) {
    if json {
        print_json(&serde_json::json!({ "app_id": app_id, "status": status }));
    }
}

/// Format a Unix timestamp (seconds) as `YYYY-MM-DD HH:MM:SS` (UTC).
pub(crate) fn format_unix_timestamp(secs: u64) -> String {
    let tod = secs % 86_400;
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    format!(
        "{} {:02}:{:02}:{:02}",
        libgaki::steam_client::unix_to_ymd(secs as i64),
        h,
        m,
        s
    )
}

/// Consume a download/verify progress stream, rendering it to the terminal.
/// In JSON mode each update is emitted as a compact NDJSON line (one object per
/// line) on stdout; the caller still prints the final result object afterward.
pub(crate) async fn drive_progress(
    rx: tokio::sync::mpsc::Receiver<DownloadProgress>,
    json: bool,
) -> Result<()> {
    drive_progress_until(rx, json).await.map(|_| ())
}

/// Like [`drive_progress`]; `false` if never completed.
pub(crate) async fn drive_progress_until(
    mut rx: tokio::sync::mpsc::Receiver<DownloadProgress>,
    json: bool,
) -> Result<bool> {
    // De-duplicate identical consecutive JSON events (the reporter ticks on a timer,
    // so it would otherwise repeat e.g. "0/0" lines while a manifest is being fetched).
    let mut last: Option<(u8, u64, u64)> = None;
    let mut rate = RateTracker::new();
    while let Some(p) = rx.recv().await {
        match p.state {
            DownloadProgressState::Queued => {
                rate.reset();
                if json {
                    emit_progress_json("queued", &p, 0.0, None, &mut last);
                } else {
                    cli_println!("Queued ...");
                }
            }
            DownloadProgressState::Downloading
            | DownloadProgressState::Verifying
            | DownloadProgressState::Moving => {
                let (state, label) = match p.state {
                    DownloadProgressState::Verifying => ("verifying", "Verifying"),
                    DownloadProgressState::Moving => ("moving", "Moving"),
                    _ => ("downloading", "Downloading"),
                };
                // Disk reuse isn't network speed.
                let (speed, eta) = rate.sample(
                    p.bytes_downloaded.saturating_sub(p.reused_bytes),
                    p.total_bytes.saturating_sub(p.reused_bytes),
                );
                if json {
                    emit_progress_json(state, &p, speed, eta, &mut last);
                } else {
                    print_progress(label, &p, speed, eta);
                }
            }
            DownloadProgressState::Completed => {
                // The caller emits the terminal result object; nothing more here.
                if !json {
                    cli_println!("\nDone.");
                }
                return Ok(true);
            }
            DownloadProgressState::Failed => {
                if !json {
                    cli_println!();
                }
                let message = format!("operation failed: {}", p.current_file);
                return Err(match p.error_kind {
                    Some(kind) => {
                        anyhow::anyhow!(libgaki::core::error::TypedError::new(kind, message))
                    }
                    None => anyhow::anyhow!(message),
                });
            }
        }
    }
    Ok(false)
}

/// Emit one compact NDJSON progress event, skipping it if identical to the last.
/// Reports the whole-app progress (`percent`), the current depot's progress
/// (`depot_percent`), and the transfer rate (`speed_bps`, bytes/sec) and
/// `eta_seconds` (null when not yet estimable).
pub(crate) fn emit_progress_json(
    state: &str,
    p: &DownloadProgress,
    speed_bps: f64,
    eta_seconds: Option<u64>,
    last: &mut Option<(u8, u64, u64)>,
) {
    // Cheap discriminator for the state so we can dedupe (state, overall, depot).
    let state_key = match state {
        "queued" => 0u8,
        "downloading" => 1,
        "verifying" => 2,
        "moving" => 3,
        _ => 4,
    };
    let key = (state_key, p.bytes_downloaded, p.depot_bytes_downloaded);
    if *last == Some(key) {
        return;
    }
    *last = Some(key);

    let value = serde_json::json!({
        "event": "progress",
        "state": state,
        // Whole-app (all depots) progress.
        "bytes_downloaded": p.bytes_downloaded,
        // Part of it verified from disk.
        "reused_bytes": p.reused_bytes,
        "total_bytes": p.total_bytes,
        "percent": percent_of(p.bytes_downloaded, p.total_bytes),
        // Current depot progress.
        "depot_id": p.depot_id,
        "depot_bytes_downloaded": p.depot_bytes_downloaded,
        "depot_total_bytes": p.depot_total_bytes,
        "depot_percent": percent_of(p.depot_bytes_downloaded, p.depot_total_bytes),
        // Rate / time remaining (for a download-manager progress bar).
        "speed_bps": speed_bps.round() as u64,
        "eta_seconds": eta_seconds,
        "file": p.current_file,
    });
    // Compact single line of NDJSON on stderr, so stdout keeps only the
    // terminal result object.
    if let Ok(s) = serde_json::to_string(&value) {
        cli_eprintln!("{s}");
    }
}

pub(crate) fn print_progress(
    phase: &str,
    p: &DownloadProgress,
    speed_bps: f64,
    eta_seconds: Option<u64>,
) {
    let overall = percent_of(p.bytes_downloaded, p.total_bytes);
    let rate = format_rate(speed_bps, eta_seconds);
    if p.depot_id != 0 {
        let depot = percent_of(p.depot_bytes_downloaded, p.depot_total_bytes);
        cli_print!(
            "\r{phase}: {overall:5.1}% overall  {}/{} bytes  | depot {}: {depot:5.1}%{rate}   ",
            p.bytes_downloaded,
            p.total_bytes,
            p.depot_id
        );
    } else {
        cli_print!(
            "\r{phase}: {overall:5.1}%  {}/{} bytes{rate}  {}   ",
            p.bytes_downloaded,
            p.total_bytes,
            p.current_file
        );
    }
    let _ = std::io::stdout().flush();
}

/// Human-readable ` 12.34 MiB/s  ETA 00:01:23` suffix (empty when no rate yet).
pub(crate) fn format_rate(speed_bps: f64, eta_seconds: Option<u64>) -> String {
    if speed_bps <= 0.0 {
        return String::new();
    }
    let mib = speed_bps / (1024.0 * 1024.0);
    match eta_seconds {
        Some(s) => format!("  {mib:6.2} MiB/s  ETA {}", format_eta(s)),
        None => format!("  {mib:6.2} MiB/s"),
    }
}

/// Format a seconds count as `HH:MM:SS`.
pub(crate) fn format_eta(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Word-wrap text to a maximum line width, preserving existing line breaks.
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
    }
    lines
}

pub(crate) fn prompt_line(prompt: &str) -> Result<String> {
    // Write the prompt to stderr so stdout stays clean (important for --json).
    cli_eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("failed reading input")?;
    Ok(input.trim().to_string())
}

#[cfg(test)]
#[path = "common_tests.rs"]
mod tests;
