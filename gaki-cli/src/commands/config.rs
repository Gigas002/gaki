//! `config` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::Result;
use libgaki::core::config::LauncherConfig;
use libgaki::ops::config::{Change, GameSettingsChange, ProxyChange};

pub(crate) async fn cmd_config_show(_json: bool) -> Result<()> {
    // The launcher configuration is structured data; it always renders as JSON.
    let config = libgaki::ops::config::load().await?;
    print_json(&config);
    Ok(())
}

/// Read the config, or with a value, set it with `mutate` and save.
/// Returns the config and whether it changed.
async fn view_or_set<T>(
    value: Option<T>,
    mutate: impl FnOnce(&mut LauncherConfig, T),
) -> Result<(LauncherConfig, bool)> {
    match value {
        Some(v) => {
            let config = libgaki::ops::config::update(|c| {
                mutate(c, v);
                Ok(())
            })
            .await?;
            Ok((config, true))
        }
        None => Ok((libgaki::ops::config::load().await?, false)),
    }
}

/// `config presence [online|offline]`: view or set the presence the daemon
/// announces for friends/chat. `offline` is an invisible presence — you appear
/// offline to friends but still sync your friends list and receive chat.
pub(crate) async fn cmd_config_presence(mode: Option<ChatPresenceArg>, json: bool) -> Result<()> {
    use libgaki::core::config::ChatPresence;
    let (config, changed) = view_or_set(mode, |c, m| c.chat_presence = m.into()).await?;
    let current = match config.chat_presence {
        ChatPresence::Online => "online",
        ChatPresence::Offline => "offline",
    };
    if json {
        print_json(&serde_json::json!({ "chat_presence": current }));
    } else {
        cli_println!("Chat presence: {current}");
        if changed {
            cli_println!(
                "Restart the session daemon for this to take effect (`aurelia daemon stop` or `aurelia kill`)."
            );
        }
    }
    Ok(())
}

/// `config experimental [true|false]`: view or set the experimental-features gate
/// that unlocks `login --openid` and `login --web-token`. See [`ConfigCommand::Experimental`].
pub(crate) async fn cmd_config_experimental(enabled: Option<bool>, json: bool) -> Result<()> {
    let changed = enabled.is_some();
    let status = libgaki::ops::config::experimental(enabled).await?;

    if json {
        print_json(&status);
    } else {
        cli_println!(
            "Experimental features: {}",
            if status.experimental {
                "enabled"
            } else {
                "disabled"
            }
        );
        if status.env_override && !status.experimental {
            cli_println!("(AURELIA_EXPERIMENTAL is set — enabled for this run regardless)");
        }
        cli_println!("Gates: login --openid, login --web-token");
        if changed {
            cli_println!("Saved.");
        }
    }
    Ok(())
}

/// `config proxy [<url>] [--no-proxy <list>] [--clear]`: view or set the network
/// proxy used for all HTTP(S) communication (Steam web endpoints, depot downloads, and
/// Proton release lookups). With no arguments, prints the current setting.
pub(crate) async fn cmd_config_proxy(
    url: Option<String>,
    no_proxy: Option<String>,
    clear: bool,
    json: bool,
) -> Result<()> {
    let change = ProxyChange {
        clear,
        url,
        no_proxy,
    };
    let changed = !change.is_noop();
    let proxy = libgaki::ops::config::update_proxy(change).await?;

    if json {
        print_json(&serde_json::json!({
            "url": proxy.url,
            "no_proxy": proxy.no_proxy,
        }));
    } else {
        match proxy.url.as_deref() {
            Some(url) => cli_println!("Proxy: {url}"),
            None => cli_println!("Proxy: (none — direct connection)"),
        }
        if let Some(no_proxy) = proxy.no_proxy.as_deref() {
            cli_println!("Bypass: {no_proxy}");
        }
        if changed {
            cli_println!(
                "Saved. Takes effect on the next command; restart the session daemon (`aurelia daemon stop`) to apply it there."
            );
        }
    }
    Ok(())
}

/// `config install-resume [manual|auto]`.
pub(crate) async fn cmd_config_install_resume(
    mode: Option<InstallResumeArg>,
    json: bool,
) -> Result<()> {
    use libgaki::library::backlog::InstallResume;
    let (config, changed) = view_or_set(mode, |c, m| c.install_resume = m.into()).await?;
    if json {
        print_json(&serde_json::json!({ "install_resume": config.install_resume }));
        return Ok(());
    }
    let label = match config.install_resume {
        InstallResume::Manual => "manual (backlog installs wait for `aurelia install resume`)",
        InstallResume::Auto => {
            "auto (the daemon resumes interrupted installs when it starts; paused and stopped ones wait)"
        }
    };
    cli_println!("Install resume: {label}");
    if changed {
        cli_println!("Saved. Applies the next time the daemon starts.");
    }
    Ok(())
}

/// `config protons`: list the Proton/Wine runtimes actually installed on disk.
/// Shares discovery with `proton list --installed` (no hardcoded placeholders).
pub(crate) async fn cmd_config_protons(json: bool) -> Result<()> {
    let listing = libgaki::ops::proton::listing().await?;
    let installed = &listing.installed;
    let steam: Vec<&str> = installed
        .iter()
        .filter(|i| i.location == "steam")
        .map(|i| i.name.as_str())
        .collect();
    let custom: Vec<&str> = installed
        .iter()
        .filter(|i| i.location != "steam")
        .map(|i| i.name.as_str())
        .collect();

    if json {
        print_json(&serde_json::json!({
            "steam": steam,
            "custom": custom,
            "default": listing.default,
        }));
        return Ok(());
    }

    if installed.is_empty() {
        cli_println!("No Proton/Wine runtimes installed.");
        cli_println!(
            "Install one with `aurelia proton install <NAME>` (see `aurelia proton list`)."
        );
        return Ok(());
    }
    if !steam.is_empty() {
        cli_println!("Steam runtimes:");
        for s in &steam {
            cli_println!("  {s}");
        }
    }
    if !custom.is_empty() {
        cli_println!("Custom (compatibilitytools.d):");
        for c in &custom {
            cli_println!("  {c}");
        }
    }
    Ok(())
}

/// `config clear-games`: reset every game's per-game settings (both stores).
pub(crate) async fn cmd_config_clear_games(yes: bool, json: bool) -> Result<()> {
    let ids = libgaki::ops::config::games_with_settings().await?;
    if ids.is_empty() {
        if json {
            print_json(&serde_json::json!({ "cleared": 0 }));
        } else {
            cli_println!("No per-game settings to clear.");
        }
        return Ok(());
    }
    confirm_write(
        "clear",
        &format!(
            "About to reset the per-game settings of {} game(s) to defaults. Continue? [y/N] ",
            ids.len()
        ),
        yes,
        json,
    )?;
    let cleared = libgaki::ops::config::clear_all_game_settings().await?;
    if json {
        print_json(&serde_json::json!({ "cleared": cleared }));
    } else {
        cli_println!("Cleared the per-game settings of {cleared} game(s).");
    }
    Ok(())
}

/// `config game`: view or set a game's per-game launch settings.
pub(crate) async fn cmd_config_game(args: GameConfigArgs, json: bool) -> Result<()> {
    let GameConfigArgs {
        app_id,
        proton,
        clear_proton,
        platform,
        no_platform,
        clear,
        launch_script,
        no_launch_script,
    } = args;

    let change = GameSettingsChange {
        reset: clear,
        proton: change(clear_proton, proton),
        platform: change(no_platform, platform.map(Into::into)),
        launch_script: change(no_launch_script, launch_script),
    };
    let (entry, _changed) = libgaki::ops::config::update_game(app_id, change).await?;
    let platforms = entry.installed_platforms;
    let platform_resolved = match platforms {
        Some((true, true)) => "linux + windows installed",
        Some((true, false)) => "linux",
        Some((false, true)) => "windows",
        Some((false, false)) => "no recognizable payload",
        None => "unknown until installed",
    };
    let runner_label = match (platforms, entry.platform_preference.as_deref()) {
        (Some((true, true)), Some("linux")) => "Native, by linux preference",
        (Some((true, true)), Some("windows")) => "Proton via Wine-TKG, by windows preference",
        (Some((true, true)), _) => "Native or Proton, no platform preference",
        (Some((true, false)), _) => "Native",
        (Some((false, true)), _) => "Proton via Wine-TKG",
        _ => "resolved at launch",
    };
    if json {
        print_json(&entry);
    } else {
        cli_println!("App {app_id}:");
        let proton_default = format!("(global default: {})", entry.default_proton_version);
        cli_println!(
            "  Proton       : {}",
            entry
                .forced_proton_version
                .as_deref()
                .unwrap_or(&proton_default)
        );
        let platform_auto = format!("(auto: {platform_resolved})");
        cli_println!(
            "  Platform     : {}",
            entry
                .platform_preference
                .as_deref()
                .unwrap_or(&platform_auto)
        );
        cli_println!("  Runner       : {runner_label}");
        cli_println!(
            "  Script       : {}",
            entry
                .launch_script
                .as_deref()
                .unwrap_or("(auto-detected / none)")
        );
    }
    Ok(())
}

/// A `--x` / `--no-x` flag pair as a setting change.
fn change<T>(clear: bool, set: Option<T>) -> Change<T> {
    match (clear, set) {
        (true, _) => Change::Clear,
        (false, Some(v)) => Change::Set(v),
        (false, None) => Change::Keep,
    }
}
