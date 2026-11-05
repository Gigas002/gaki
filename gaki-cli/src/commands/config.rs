//! `config` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::Result;
use libgaki::config::Config;
use libgaki::core::config::LauncherConfig;
use libgaki::ops::config::{Change, GameEntryChange, ProxyChange};

/// `config show`: the `config.toml` settings, defaults filled in.
pub(crate) async fn cmd_config_show(json: bool) -> Result<()> {
    let config = libgaki::ops::config::global()?;
    if json {
        print_json(&config);
        return Ok(());
    }
    let path = libgaki::core::config::config_dir()?.join(libgaki::config::CONFIG_FILE);
    cli_println!("# {}", path.display());
    cli_print!("{}", config.to_toml());
    Ok(())
}

/// `config check`: load and validate every config file.
pub(crate) async fn cmd_config_check(json: bool) -> Result<()> {
    let report = libgaki::ops::config::check()?;
    if json {
        let issues: Vec<_> = report
            .issues
            .iter()
            .map(|issue| {
                serde_json::json!({
                    "severity": issue.severity.to_string(),
                    "subject": issue.subject.to_string(),
                    "message": issue.message,
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "load_errors": report.load_errors,
            "issues": issues,
        }));
    } else if report.load_errors.is_empty() && report.issues.is_empty() {
        cli_println!("No problems found.");
    } else {
        for error in &report.load_errors {
            cli_println!("error: {error}");
        }
        for issue in &report.issues {
            cli_println!("{issue}");
        }
    }
    if report.has_errors() {
        anyhow::bail!("the configuration has errors");
    }
    Ok(())
}

/// `config get KEY`.
pub(crate) async fn cmd_config_get(key: &str, json: bool) -> Result<()> {
    let value = libgaki::ops::config::get_global(key)?;
    if json {
        print_json(&serde_json::json!({ "key": key, "value": value }));
    } else if let Some(value) = value {
        cli_println!("{value}");
    }
    Ok(())
}

/// `config set KEY VALUE` / `config unset KEY`.
pub(crate) async fn cmd_config_set(key: &str, value: Option<&str>, json: bool) -> Result<()> {
    let (_, changed) = libgaki::ops::config::set_global(key, value)?;
    let now = libgaki::ops::config::get_global(key)?;
    if json {
        print_json(&serde_json::json!({ "key": key, "value": now, "changed": changed }));
        return Ok(());
    }
    match (&now, changed) {
        (Some(value), true) => cli_println!("{key} = {value}"),
        (None, true) => cli_println!("{key} unset."),
        (_, false) => cli_println!("{key}: unchanged."),
    }
    if changed && daemon_reads(key) {
        cli_println!(
            "Restart the session daemon for this to apply there (`gaki-cli daemon stop`)."
        );
    }
    Ok(())
}

/// Keys the session daemon reads once, at start.
fn daemon_reads(key: &str) -> bool {
    matches!(key, "presence" | "proxy" | "no_proxy" | "install_resume")
}

/// `config presence [online|offline]`: view or set the presence the daemon
/// announces for friends/chat. `offline` is an invisible presence — you appear
/// offline to friends but still sync your friends list and receive chat.
pub(crate) async fn cmd_config_presence(mode: Option<ChatPresenceArg>, json: bool) -> Result<()> {
    use libgaki::core::config::ChatPresence;
    let changed = mode.is_some();
    let config = match mode {
        Some(mode) => libgaki::ops::config::set_presence(mode.into())?,
        None => libgaki::ops::config::global()?,
    };
    let current = match config.presence {
        ChatPresence::Online => "online",
        ChatPresence::Offline => "offline",
    };
    if json {
        print_json(&serde_json::json!({ "chat_presence": current }));
    } else {
        cli_println!("Chat presence: {current}");
        if changed {
            cli_println!(
                "Restart the session daemon for this to take effect (`gaki-cli daemon stop` or `gaki-cli kill`)."
            );
        }
    }
    Ok(())
}

/// Read `config.toml`, or with a value, set `key` to it first.
fn view_or_set(key: &str, value: Option<String>) -> Result<(Config, bool)> {
    match value {
        Some(value) => libgaki::ops::config::set_global(key, Some(&value)),
        None => Ok((libgaki::ops::config::global()?, false)),
    }
}

/// Apply a [`Change`] to an optional `config.toml` key.
fn set_optional(key: &str, change: Change<String>) -> Result<(Config, bool)> {
    match change {
        Change::Keep => Ok((libgaki::ops::config::global()?, false)),
        Change::Clear => libgaki::ops::config::set_global(key, None),
        Change::Set(value) => libgaki::ops::config::set_global(key, Some(&value)),
    }
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
                "Saved. Takes effect on the next command; restart the session daemon (`gaki-cli daemon stop`) to apply it there."
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
    let value = mode.map(|m| match InstallResume::from(m) {
        InstallResume::Manual => "manual".to_owned(),
        InstallResume::Auto => "auto".to_owned(),
    });
    let (config, changed) = view_or_set("install_resume", value)?;
    if json {
        print_json(&serde_json::json!({ "install_resume": config.install_resume }));
        return Ok(());
    }
    let label = match config.install_resume {
        InstallResume::Manual => "manual (backlog installs wait for `gaki-cli install resume`)",
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

/// `config keyring [true|false]`.
pub(crate) async fn cmd_config_keyring(enabled: Option<bool>, json: bool) -> Result<()> {
    // Still in config.json until the credential store replaces it (5.9).
    let changed = enabled.is_some();
    let config = match enabled {
        Some(value) => {
            libgaki::ops::config::update(|c: &mut LauncherConfig| {
                c.use_keyring = value;
                Ok(())
            })
            .await?
        }
        None => libgaki::ops::config::load().await?,
    };
    if json {
        print_json(&serde_json::json!({ "use_keyring": config.use_keyring }));
        return Ok(());
    }
    cli_println!(
        "OS keyring: {}",
        if config.use_keyring {
            "used for the session key"
        } else {
            "not used"
        }
    );
    if changed {
        cli_println!("Saved.");
    }
    Ok(())
}

/// `config script-dir [PATH|--clear]`.
pub(crate) async fn cmd_config_script_dir(change: Change<String>, json: bool) -> Result<()> {
    let (config, changed) = set_optional("script_dir", change)?;
    let effective = libgaki::launch::launch_script::script_dir();
    if json {
        print_json(&serde_json::json!({
            "script_dir": config.script_dir,
            "effective": effective.display().to_string(),
        }));
        return Ok(());
    }
    match &config.script_dir {
        Some(dir) => cli_println!("Script dir: {}", dir.display()),
        None => cli_println!("Script dir: {} (default)", effective.display()),
    }
    if changed {
        cli_println!("Saved.");
    }
    Ok(())
}

/// `config info-cache-ttl [SECONDS|--clear]`.
pub(crate) async fn cmd_config_info_cache_ttl(change: Change<u64>, json: bool) -> Result<()> {
    let (config, changed) = set_optional("info_cache_ttl", change.map(|s| s.to_string()))?;
    let effective = config
        .info_cache_ttl
        .unwrap_or(libgaki::core::config::INFO_CACHE_DEFAULT_TTL_SECS);
    if json {
        print_json(&serde_json::json!({
            "info_cache_ttl": config.info_cache_ttl,
            "effective_seconds": effective,
        }));
        return Ok(());
    }
    let suffix = if config.info_cache_ttl.is_none() {
        " (default)"
    } else {
        ""
    };
    if effective == 0 {
        cli_println!("Info cache: disabled{suffix}");
    } else {
        cli_println!("Info cache TTL: {effective}s{suffix}");
    }
    if changed {
        cli_println!("Saved.");
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
            "Install one with `gaki-cli proton install <NAME>` (see `gaki-cli proton list`)."
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

/// `config game`: view or set a game's `games.toml` entry.
pub(crate) async fn cmd_config_game(args: GameConfigArgs, json: bool) -> Result<()> {
    let GameConfigArgs {
        app_id,
        profile,
        no_profile,
        platform,
        no_platform,
        script,
        no_script,
        steam,
        no_steam,
        name,
        no_name,
        clear,
    } = args;

    let change = GameEntryChange {
        reset: clear,
        name: change(no_name, name),
        profile: change(no_profile, profile),
        platform: change(
            no_platform,
            platform.map(|p| {
                libgaki::config::Platform::from(libgaki::core::models::DepotPlatform::from(p))
            }),
        ),
        script: change(no_script, script),
        steam: change(no_steam, steam.map(Into::into)),
    };
    let (game, changed) = libgaki::ops::config::update_game_entry(app_id, change).await?;
    if json {
        print_json(&game);
        return Ok(());
    }
    let entry = game.entry.as_ref();
    let set_or = |value: Option<String>, fallback: String| match value {
        Some(value) => value,
        None => format!("({fallback})"),
    };
    let installed = match game.installed_platforms {
        Some((true, true)) => "linux + windows installed",
        Some((true, false)) => "linux installed",
        Some((false, true)) => "windows installed",
        Some((false, false)) => "no recognizable payload",
        None => "not installed",
    };
    cli_println!("App {app_id}:");
    cli_println!(
        "  Name     : {}",
        set_or(entry.and_then(|e| e.name.clone()), "no alias".into())
    );
    cli_println!(
        "  Profile  : {}",
        set_or(
            entry.and_then(|e| e.profile.clone()),
            match &game.profile {
                Some(p) => format!("default: {p}"),
                None => "none".into(),
            }
        )
    );
    cli_println!(
        "  Platform : {}",
        set_or(
            entry.and_then(|e| e.platform).map(|p| p.to_string()),
            match game.platform {
                Some(p) => format!("resolved: {p}"),
                None => format!("auto: {installed}"),
            }
        )
    );
    cli_println!(
        "  Steam    : {}",
        set_or(
            entry.and_then(|e| e.steam).map(|s| s.to_string()),
            format!("resolved: {}", game.steam)
        )
    );
    cli_println!(
        "  Script   : {}",
        set_or(
            entry
                .and_then(|e| e.script.as_ref())
                .map(|p| p.display().to_string()),
            match &game.script {
                Some(p) => format!("fallback: {}", p.display()),
                None => "none".into(),
            }
        )
    );
    if changed {
        cli_println!("Saved.");
    }
    Ok(())
}

/// A `--x` / `--no-x` flag pair as a setting change.
pub(crate) fn change<T>(clear: bool, set: Option<T>) -> Change<T> {
    match (clear, set) {
        (true, _) => Change::Clear,
        (false, Some(v)) => Change::Set(v),
        (false, None) => Change::Keep,
    }
}
