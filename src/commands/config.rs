//! `config` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::{Context, Result};
use aurelia::core::config::load_launcher_config;
use aurelia::core::config::save_launcher_config;

pub(crate) async fn cmd_config_show(_json: bool) -> Result<()> {
    // The launcher configuration is structured data; it always renders as JSON.
    let config = load_launcher_config().await?;
    print_json(&config);
    Ok(())
}

/// Load config; optionally mutate, save.
async fn view_or_set<T>(
    value: Option<T>,
    mutate: impl FnOnce(&mut aurelia::core::config::LauncherConfig, T),
) -> Result<(aurelia::core::config::LauncherConfig, bool)> {
    let mut config = load_launcher_config().await?;
    let changed = value.is_some();
    if let Some(v) = value {
        mutate(&mut config, v);
        save_launcher_config(&config).await?;
    }
    Ok((config, changed))
}

/// `config presence [online|offline]`: view or set the presence the daemon
/// announces for friends/chat. `offline` is an invisible presence — you appear
/// offline to friends but still sync your friends list and receive chat.
pub(crate) async fn cmd_config_presence(mode: Option<ChatPresenceArg>, json: bool) -> Result<()> {
    use aurelia::core::config::ChatPresence;
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
    let (config, changed) = view_or_set(enabled, |c, value| c.experimental = value).await?;
    // The env var forces experimental on for a single run regardless of the file.
    let env_override = std::env::var_os("AURELIA_EXPERIMENTAL").is_some_and(|v| {
        let v = v.to_string_lossy();
        let v = v.trim();
        !v.is_empty() && !v.eq_ignore_ascii_case("0") && !v.eq_ignore_ascii_case("false")
    });
    let effective = config.experimental || env_override;

    if json {
        print_json(&serde_json::json!({
            "experimental": config.experimental,
            "env_override": env_override,
            "effective": effective,
        }));
    } else {
        cli_println!(
            "Experimental features: {}",
            if config.experimental {
                "enabled"
            } else {
                "disabled"
            }
        );
        if env_override && !config.experimental {
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
    use aurelia::core::net::validate_proxy_url;

    let mut config = load_launcher_config().await?;
    let changed = clear || url.is_some() || no_proxy.is_some();

    if clear {
        config.proxy.url = None;
        config.proxy.no_proxy = None;
    } else {
        if let Some(url) = url {
            let value = url.trim();
            if value.is_empty() {
                config.proxy.url = None;
            } else {
                validate_proxy_url(value)?;
                config.proxy.url = Some(value.to_string());
            }
        }
        if let Some(no_proxy) = no_proxy {
            let value = no_proxy.trim();
            config.proxy.no_proxy = (!value.is_empty()).then(|| value.to_string());
        }
    }

    if changed {
        save_launcher_config(&config)
            .await
            .context("failed saving proxy config")?;
    }

    if json {
        print_json(&serde_json::json!({
            "url": config.proxy.url,
            "no_proxy": config.proxy.no_proxy,
        }));
    } else {
        match config.proxy.url.as_deref() {
            Some(url) => cli_println!("Proxy: {url}"),
            None => cli_println!("Proxy: (none — direct connection)"),
        }
        if let Some(no_proxy) = config.proxy.no_proxy.as_deref() {
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
    use aurelia::library::backlog::InstallResume;
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
    let cfg = load_launcher_config().await?;
    let installed =
        aurelia::compat::proton::list_installed(std::path::Path::new(&cfg.steam_library_path));
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
            "default": cfg.proton_version,
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
    let mut cfg = load_launcher_config().await?;
    let mut user_configs = aurelia::core::config::load_user_configs().await?;
    let ids: std::collections::BTreeSet<u32> = cfg
        .game_configs
        .keys()
        .chain(user_configs.keys())
        .copied()
        .collect();
    if ids.is_empty() {
        if json {
            print_json(&serde_json::json!({ "cleared": 0 }));
        } else {
            cli_println!("No per-game settings to clear.");
        }
        return Ok(());
    }
    crate::commands::common::confirm_write(
        "clear",
        &format!(
            "About to reset the per-game settings of {} game(s) to defaults. Continue? [y/N] ",
            ids.len()
        ),
        yes,
        json,
    )?;
    cfg.game_configs.clear();
    user_configs.clear();
    cfg.save().await.context("failed saving game config")?;
    aurelia::core::config::save_user_configs(&user_configs)
        .await
        .context("failed saving per-game config")?;
    if json {
        print_json(&serde_json::json!({ "cleared": ids.len() }));
    } else {
        cli_println!("Cleared the per-game settings of {} game(s).", ids.len());
    }
    Ok(())
}

/// Which platform payloads are installed: (linux, windows).
async fn installed_platform_set(app_id: u32) -> Option<(bool, bool)> {
    let installed = aurelia::library::scan_installed_app_info().await.ok()?;
    let info = installed.get(&app_id)?;
    crate::commands::library::detect_installed_platform_set(&info.install_path.to_string_lossy())
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

    // `--clear` also resets the per-game launch tweaks kept in the separate store
    // (user_apps.json: env, launch options, graphics layers).
    if clear {
        let mut user_configs = aurelia::core::config::load_user_configs().await?;
        if user_configs.remove(&app_id).is_some() {
            aurelia::core::config::save_user_configs(&user_configs)
                .await
                .context("failed saving per-game config")?;
        }
    }

    let mut cfg = load_launcher_config().await?;
    let mut changed = false;
    if clear {
        changed = cfg.game_configs.remove(&app_id).is_some();
    } else {
        let entry = cfg.game_configs.entry(app_id).or_default();
        if clear_proton {
            entry.forced_proton_version = None;
            changed = true;
        } else if let Some(p) = proton {
            entry.forced_proton_version = Some(p);
            changed = true;
        }
        if no_platform {
            entry.platform_preference = None;
            changed = true;
        } else if let Some(pl) = platform {
            entry.platform_preference = Some(
                match pl {
                    PlatformArg::Windows => "windows",
                    PlatformArg::Linux => "linux",
                }
                .to_string(),
            );
            changed = true;
        }
        if no_launch_script {
            entry.launch_script = None;
            changed = true;
        } else if let Some(s) = launch_script {
            entry.launch_script = Some(s.to_string_lossy().to_string());
            changed = true;
        }
    }
    if changed {
        cfg.save().await.context("failed saving game config")?;
    }

    let entry = cfg.game_configs.get(&app_id).cloned().unwrap_or_default();
    let platforms = installed_platform_set(app_id).await;
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
        print_json(&serde_json::json!({
            "app_id": app_id,
            "forced_proton_version": entry.forced_proton_version,
            "platform_preference": entry.platform_preference,
            "launch_script": entry.launch_script,
        }));
    } else {
        cli_println!("App {app_id}:");
        let proton_default = format!("(global default: {})", cfg.proton_version);
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
