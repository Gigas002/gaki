//! `info` command handlers.

use crate::commands::common::*;

use anyhow::Result;
use libgaki::ops::info::{self, AppInfo, ExtendedInfo, InfoRequest};
use libgaki::steam_client::SteamClient;
use std::path::PathBuf;

pub(crate) async fn cmd_set_dlc(app_id: u32, enable: bool, restart_steam: bool) -> Result<()> {
    let client = restored_client().await?;

    // Steam flushes its in-memory app state on exit, so the edit must happen while
    // Steam is stopped to survive. With --restart-steam: stop → edit → start.
    let manage_steam = steam_guard_stop_optional(restart_steam)?;

    let base = info::set_dlc(&client, app_id, enable).await?;

    steam_guard_restart(manage_steam)?;

    let action = if enable { "enabled" } else { "disabled" };
    let restart_required = !manage_steam && SteamClient::steam_is_running();

    cli_println!("DLC {app_id} {action} for base game {base}.");
    if enable {
        cli_println!(
            "(Toggles the flag only — run `gaki-cli install {app_id}` if the content isn't downloaded.)"
        );
    }
    if restart_required {
        cli_eprintln!(
            "Note: Steam is running and reads DLC state only at startup, so this won't apply"
        );
        cli_eprintln!("      until you restart Steam (or re-run with --restart-steam).");
    }
    Ok(())
}

#[cfg(feature = "versioning")]
pub(crate) async fn cmd_branches(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let branches = info::branches(&client, app_id).await?;
    if branches.is_empty() {
        cli_println!("No branches reported.");
    } else {
        for b in branches {
            cli_println!("{b}");
        }
    }
    Ok(())
}

#[cfg(feature = "versioning")]
pub(crate) async fn cmd_set_branch(app_id: u32, branch: String) -> Result<()> {
    let client = authed_client().await?;
    info::set_branch(&client, app_id, &branch).await?;
    cli_println!("App {app_id} set to branch '{branch}'. Run `gaki-cli update {app_id}` to apply.");

    Ok(())
}

pub(crate) async fn cmd_info(
    app_ids: Vec<u32>,
    extended: bool,
    no_cache: bool,
    country: Option<String>,
) -> Result<()> {
    // Country selects the price region and cache key.
    let request = InfoRequest {
        app_ids,
        extended,
        no_cache,
        country: libgaki::ops::info::resolve_country(country.as_deref())?,
    };
    let infos = info::app_info(&request, authed_client).await?;

    for (i, app) in infos.iter().enumerate() {
        if i > 0 {
            cli_println!("\n{}", "─".repeat(60));
        }
        print_info_human(app);
    }
    Ok(())
}

/// Render the human-readable `info` block for one app to stdout.
pub(crate) fn print_info_human(app: &AppInfo) {
    let AppInfo {
        details,
        dlc,
        extended,
    } = app;
    let extended_info: Option<&ExtendedInfo> = extended.as_ref();
    // --- Header ---
    let ea = if details.is_early_access {
        " [Early Access]"
    } else {
        ""
    };
    cli_println!("{}  (app {}){ea}", details.name, details.app_id);
    if !details.app_type.is_empty() {
        cli_println!("Type       : {}", details.app_type);
    }
    if !details.developers.is_empty() {
        cli_println!("Developers : {}", details.developers.join(", "));
    }
    if !details.publishers.is_empty() {
        cli_println!("Publishers : {}", details.publishers.join(", "));
    }
    if !details.franchises.is_empty() {
        cli_println!("Franchises : {}", details.franchises.join(", "));
    }
    if let Some(date) = &details.release_date {
        let suffix = if details.coming_soon {
            " (coming soon)"
        } else {
            ""
        };
        cli_println!("Released   : {date}{suffix}");
    }
    if let Some(price) = &details.price {
        let discount = if details.discount_pct > 0 {
            let mut note = format!(" (-{}%", details.discount_pct);
            if let Some(orig) = &details.original_price {
                note.push_str(&format!(", was {orig}"));
            }
            if let Some(end) = details.discount_end {
                note.push_str(&format!(
                    ", until {}",
                    libgaki::steam_client::unix_to_ymd(end as i64)
                ));
            }
            note.push(')');
            note
        } else {
            String::new()
        };
        let region = if details.country.is_empty() {
            String::new()
        } else {
            format!("  [{}]", details.country)
        };
        cli_println!("Price      : {price}{discount}{region}");
    }
    if details.region_locked {
        cli_println!("Region     : not sold in {}", details.country);
    }
    if !details.platforms.is_empty() {
        cli_println!("Platforms  : {}", details.platforms.join(", "));
    }
    if let Some(reviews) = &details.review_summary {
        cli_println!("Reviews    : {reviews}");
    }
    let tag_names: Vec<&str> = details
        .tags
        .iter()
        .filter_map(|t| t.name.as_deref())
        .take(20)
        .collect();
    if !tag_names.is_empty() {
        cli_println!("Tags       : {}", tag_names.join(", "));
    }
    if let Some(rating) = &details.rating {
        let descriptors = if rating.descriptors.is_empty() {
            String::new()
        } else {
            format!(" ({})", rating.descriptors.join(", "))
        };
        cli_println!(
            "Rating     : {} {}{descriptors}",
            rating.system.to_uppercase(),
            rating.rating.to_uppercase()
        );
    }
    if let Some(web) = extended_info {
        if let Some(score) = web.metacritic {
            cli_println!("Metacritic : {score}");
        }
        if let Some(site) = &web.website {
            cli_println!("Website    : {site}");
        }
    }

    // --- Description ---
    if !details.short_description.is_empty() {
        cli_println!("\nDescription:");
        for line in wrap_text(&details.short_description, 88) {
            cli_println!("  {line}");
        }
    }

    // --- Extended: genres / categories / requirements ---
    if let Some(web) = extended_info {
        if !web.genres.is_empty() {
            cli_println!("Genres    : {}", web.genres.join(", "));
        }
        if !web.categories.is_empty() {
            cli_println!("Categories: {}", web.categories.join(", "));
        }
        if !web.requirements_minimum.is_empty() {
            cli_println!("\nMinimum requirements:");
            for line in &web.requirements_minimum {
                cli_println!("  {line}");
            }
        }
        if !web.requirements_recommended.is_empty() {
            cli_println!("\nRecommended requirements:");
            for line in &web.requirements_recommended {
                cli_println!("  {line}");
            }
        }
    }

    // --- Artwork ---
    let a = &details.assets;
    if a.header.is_some() || a.capsule.is_some() || a.background.is_some() {
        cli_println!("\nArtwork:");
        if let Some(u) = &a.header {
            cli_println!("  header    : {u}");
        }
        if let Some(u) = &a.capsule {
            cli_println!("  capsule   : {u}");
        }
        if let Some(u) = &a.hero {
            cli_println!("  hero      : {u}");
        }
        if let Some(u) = &a.background {
            cli_println!("  background: {u}");
        }
        if let Some(u) = &a.logo {
            cli_println!("  logo      : {u}");
        }
    }

    // --- DLC ---
    if !dlc.is_empty() {
        cli_println!("\nDLC ({}):", dlc.len());
        for (id, name) in dlc {
            let name = name
                .clone()
                .unwrap_or_else(|| "(name unavailable)".to_string());
            cli_println!("  {id:>9}  {name}");
        }
    }
}

pub(crate) async fn cmd_dlc(app_id: u32) -> Result<()> {
    // Ownership needs a session; installed/disabled come from the appmanifest.
    let steam = authed_client().await?;
    let dlc = info::dlc_list(&steam, app_id).await?;

    if dlc.is_empty() {
        cli_println!("No DLC for app {app_id}.");
        return Ok(());
    }
    cli_println!(
        "{:>9}  {:<5}  {:<13}  {:<16}  NAME",
        "APPID",
        "OWNED",
        "STATUS",
        "PRICE"
    );
    for d in &dlc {
        let id = d.app_id;
        let name = d.name.as_deref().unwrap_or("(name unavailable)");
        let price = d
            .store
            .as_ref()
            .map(|i| {
                let p = i.price.clone().unwrap_or_else(|| "-".to_string());
                if i.discount_pct > 0 {
                    format!("{p} (-{}%)", i.discount_pct)
                } else {
                    p
                }
            })
            .unwrap_or_else(|| "-".to_string());
        let s = d.state.as_ref();
        let owned = match s.map(|s| s.owned) {
            Some(true) => "yes",
            Some(false) => "no",
            None => "?",
        };
        // Installed/disabled describe the local content state of an owned DLC.
        let status = match s {
            Some(s) if !s.installed => "not-installed",
            Some(s) if s.disabled => "disabled",
            Some(_) => "enabled",
            None => "?",
        };
        cli_println!("{id:>9}  {owned:<5}  {status:<13}  {price:<16}  {name}");
    }
    Ok(())
}

pub(crate) async fn cmd_drm(app_id: u32) -> Result<()> {
    use libgaki::steam_client::EncryptedTicketOutcome;

    let client = authed_client().await?;
    let info::DrmReport {
        name,
        ownership,
        encrypted,
    } = info::drm_check(&client, app_id).await;

    let title = name.unwrap_or_else(|| format!("app {app_id}"));
    cli_println!("DRM verification for {title} ({app_id})");
    match &ownership {
        Ok(len) => {
            cli_println!("  ownership ticket:     issued ({len} bytes) — account holds a license")
        }
        Err(e) => cli_println!("  ownership ticket:     not issued — {e:#}"),
    }
    match &encrypted {
        Ok(EncryptedTicketOutcome::Issued(t)) => {
            cli_println!("  encrypted app ticket: issued ({} bytes)", t.len());
        }
        Ok(EncryptedTicketOutcome::Refused(2)) => cli_println!(
            "  encrypted app ticket: refused (eresult 2) — the app likely has no encrypted-ticket key configured"
        ),
        Ok(EncryptedTicketOutcome::Refused(25)) => cli_println!(
            "  encrypted app ticket: refused (eresult 25) — rate limited, retry in a minute"
        ),
        Ok(EncryptedTicketOutcome::Refused(r)) => {
            cli_println!("  encrypted app ticket: refused (eresult {r})");
        }
        Err(e) => cli_println!("  encrypted app ticket: error — {e:#}"),
    }
    Ok(())
}

pub(crate) async fn cmd_achievements(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let achievements = info::achievements(&client, app_id).await?;
    let unlocked = achievements.iter().filter(|a| a.unlocked).count();

    if achievements.is_empty() {
        cli_println!("No achievements for app {app_id}.");
        return Ok(());
    }
    cli_println!("{:<2}  {:>6}  {:<19}  NAME", "", "RARITY", "UNLOCKED");
    for a in &achievements {
        let mark = if a.unlocked { "✓" } else { " " };
        let when = a
            .unlock_time
            .map(|t| format_unix_timestamp(u64::from(t)))
            .unwrap_or_else(|| "-".to_string());
        let name = if a.hidden && !a.unlocked {
            format!("{} (hidden)", a.name)
        } else {
            a.name.clone()
        };
        cli_println!("{mark:<2}  {:>5.1}%  {when:<19}  {name}", a.global_percent);
    }
    cli_println!("\n{unlocked}/{} unlocked.", achievements.len());
    Ok(())
}

pub(crate) async fn cmd_depots(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let depots = info::depots(&client, app_id).await?;

    if depots.is_empty() {
        cli_println!("No depots reported.");
        return Ok(());
    }
    cli_println!("{:>12}  {:>14}  NAME", "DEPOT", "SIZE(bytes)");
    for d in &depots {
        cli_println!("{:>12}  {:>14}  {}", d.id, d.size, d.name);
    }
    Ok(())
}

pub(crate) async fn cmd_launch_options(app_id: u32) -> Result<()> {
    let client = authed_client().await?;
    let options = info::launch_options(&client, app_id).await?;

    if options.is_empty() {
        cli_println!("No launch options for app {app_id}.");
        return Ok(());
    }
    cli_println!("{:>3}  {:<10}  NAME / COMMAND", "ID", "OS");
    for o in &options {
        let os = if o.oslist.is_empty() {
            "any"
        } else {
            &o.oslist
        };
        let desc = if o.description.is_empty() {
            &o.executable
        } else {
            &o.description
        };
        cli_println!("{:>3}  {:<10}  {}", o.id, os, desc);
        let cmd = format!("{} {}", o.executable, o.arguments);
        let cmd = cmd.trim();
        if !cmd.is_empty() {
            cli_println!("       {cmd}");
        }
    }
    Ok(())
}

pub(crate) async fn cmd_image(app_id: u32, output: Option<PathBuf>, force: bool) -> Result<()> {
    let final_path = info::artwork(app_id, output.as_deref(), force).await?;
    cli_println!("{}", final_path.display());

    Ok(())
}

/// `gaki-cli image APPID --list [--probe]`: every asset URL.
pub(crate) async fn cmd_image_list(app_id: u32, probe: bool) -> Result<()> {
    let info = info::record_with_media(app_id, authed_client).await?;
    let mut entries = info::assets(&info);
    if probe {
        info::probe_assets(&mut entries).await;
    }

    cli_println!("{}  (app {app_id}): {} assets", info.name, entries.len());
    for info::Asset { kind, url, status } in &entries {
        let status = match status {
            Some(s) => s.to_string(),
            None if probe => "ERR".to_string(),
            None => "-".to_string(),
        };
        cli_println!("{kind:<14} {status:<4} {url}");
    }
    if probe {
        let bad = entries
            .iter()
            .filter(|a| !matches!(a.status, Some(200..=299)))
            .count();
        cli_println!("\n{} of {} URLs failed the probe", bad, entries.len());
    }
    Ok(())
}

/// `gaki-cli appinfo APPID`: raw PICS `common`/`extended` sections.
pub(crate) async fn cmd_appinfo(app_id: u32) -> Result<()> {
    crate::commands::auth::require_experimental("appinfo").await?;
    let client = authed_client().await?;
    cli_println!(
        "{}",
        libgaki::ops::info::appinfo_debug(&client, app_id).await?
    );
    Ok(())
}

/// `gaki-cli tags [--dump]`: the store tag vocabulary.
pub(crate) async fn cmd_tags(dump: bool) -> Result<()> {
    crate::commands::auth::require_experimental("tags").await?;
    let client = authed_client().await?;
    let tags = info::tags(&client).await?;
    if !dump {
        for (id, name) in &tags {
            cli_println!("{id:>6}  {name}");
        }
        return Ok(());
    }
    let today = libgaki::steam_client::unix_to_ymd(libgaki::core::utils::now_unix() as i64);
    cli_println!("//! Generated by `gaki-cli tags --dump`; do not edit.\n");
    cli_println!("/// Snapshot date of the vocabulary below.");
    cli_println!("pub const TAGS_GENERATED_AT: &str = \"{today}\";\n");
    cli_println!(
        "/// One store tag.\npub struct TagDef {{\n    pub id: u32,\n    pub name: &'static str,\n}}\n"
    );
    cli_println!("/// Sorted by `id`.\npub static TAGS: &[TagDef] = &[");
    for (id, name) in &tags {
        cli_println!("    TagDef {{ id: {id}, name: {:?} }},", name);
    }
    cli_println!("];\n");
    cli_println!(
        "/// English name for a tag id.\npub fn tag_name(id: u32) -> Option<&'static str> {{"
    );
    cli_println!(
        "    TAGS.binary_search_by_key(&id, |t| t.id)\n        .ok()\n        .map(|i| TAGS[i].name)\n}}"
    );
    Ok(())
}
