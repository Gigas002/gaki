//! `config` command handlers.

use crate::cli::*;
use crate::commands::common::*;

use anyhow::Result;
use libgaki::config::Config;
use libgaki::ops::config::{Change, EnvChange, GameEntryChange, ProxyChange};

/// `config show`: the `config.toml` settings, defaults filled in.
pub(crate) async fn cmd_config_show() -> Result<()> {
    let config = libgaki::ops::config::global()?;
    let path = libgaki::core::config::config_dir()?.join(libgaki::config::CONFIG_FILE);
    cli_println!("# {}", path.display());
    cli_print!("{}", config.to_toml());
    Ok(())
}

/// `config check`: load and validate every config file.
pub(crate) async fn cmd_config_check() -> Result<()> {
    let report = libgaki::ops::config::check()?;
    if report.load_errors.is_empty() && report.issues.is_empty() {
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
pub(crate) async fn cmd_config_get(key: &str) -> Result<()> {
    let value = libgaki::ops::config::get_global(key)?;
    if let Some(value) = value {
        cli_println!("{value}");
    }
    Ok(())
}

/// `config set KEY VALUE` / `config unset KEY`.
pub(crate) async fn cmd_config_set(key: &str, value: Option<&str>) -> Result<()> {
    let (_, changed) = libgaki::ops::config::set_global(key, value)?;
    let now = libgaki::ops::config::get_global(key)?;
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
pub(crate) async fn cmd_config_presence(mode: Option<ChatPresenceArg>) -> Result<()> {
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
    cli_println!("Chat presence: {current}");
    if changed {
        cli_println!(
            "Restart the session daemon for this to take effect (`gaki-cli daemon stop` or `gaki-cli kill`)."
        );
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
pub(crate) async fn cmd_config_experimental(enabled: Option<bool>) -> Result<()> {
    let changed = enabled.is_some();
    let status = libgaki::ops::config::experimental(enabled).await?;

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

    Ok(())
}

/// `config proxy [<url>] [--no-proxy <list>] [--clear]`: view or set the network
/// proxy used for all HTTP(S) communication (Steam web endpoints, depot downloads, and
/// Proton release lookups). With no arguments, prints the current setting.
pub(crate) async fn cmd_config_proxy(
    url: Option<String>,
    no_proxy: Option<String>,
    clear: bool,
) -> Result<()> {
    let change = ProxyChange {
        clear,
        url,
        no_proxy,
    };
    let changed = !change.is_noop();
    let proxy = libgaki::ops::config::update_proxy(change).await?;

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

    Ok(())
}

/// `config install-resume [manual|auto]`.
pub(crate) async fn cmd_config_install_resume(mode: Option<InstallResumeArg>) -> Result<()> {
    use libgaki::library::backlog::InstallResume;
    let value = mode.map(|m| match InstallResume::from(m) {
        InstallResume::Manual => "manual".to_owned(),
        InstallResume::Auto => "auto".to_owned(),
    });
    let (config, changed) = view_or_set("install_resume", value)?;
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

/// `config info-cache-ttl [SECONDS|--clear]`.
pub(crate) async fn cmd_config_info_cache_ttl(change: Change<u64>) -> Result<()> {
    let (config, changed) = set_optional("info_cache_ttl", change.map(|s| s.to_string()))?;
    let effective = config
        .info_cache_ttl
        .unwrap_or(libgaki::core::config::INFO_CACHE_DEFAULT_TTL_SECS);
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
pub(crate) async fn cmd_config_protons() -> Result<()> {
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
pub(crate) async fn cmd_config_clear_games(yes: bool) -> Result<()> {
    let ids = libgaki::ops::config::games_with_settings().await?;
    if ids.is_empty() {
        cli_println!("No per-game settings to clear.");

        return Ok(());
    }
    confirm_write(
        &format!(
            "About to reset the per-game settings of {} game(s) to defaults. Continue? [y/N] ",
            ids.len()
        ),
        yes,
    )?;
    let cleared = libgaki::ops::config::clear_all_game_settings().await?;
    cli_println!("Cleared the per-game settings of {cleared} game(s).");

    Ok(())
}

/// `config game`: view or set a game's `games.toml` entry.
pub(crate) async fn cmd_config_game(args: GameConfigArgs) -> Result<()> {
    let GameConfigArgs {
        app_id,
        profile,
        no_profile,
        platform,
        no_platform,
        wrappers,
        no_wrappers,
        steam,
        no_steam,
        name,
        no_name,
        args,
        no_args,
        env,
        unset_env,
        no_env,
        shader_cache,
        no_shader_cache,
        clear,
    } = args;

    let set = env
        .iter()
        .map(|var| libgaki::ops::config::parse_env_var(var))
        .collect::<Result<_>>()?;

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
        wrappers: change(no_wrappers, (!wrappers.is_empty()).then_some(wrappers)),
        steam: change(no_steam, steam.map(Into::into)),
        args: change(no_args, (!args.is_empty()).then_some(args)),
        env: EnvChange {
            clear: no_env,
            unset: unset_env,
            set,
        },
        shader_cache: change(
            no_shader_cache,
            shader_cache.map(|text| libgaki::config::ShaderCache::parse(&text)),
        ),
    };
    let (game, changed) = libgaki::ops::config::update_game_entry(app_id, change).await?;
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
    let own = entry.map(|e| e.wrappers.len()).unwrap_or(0);
    cli_println!(
        "  Wrappers : {}",
        if game.wrappers.is_empty() {
            "(none)".to_owned()
        } else if own == 0 {
            format!("(profile: {})", game.wrappers.join(" "))
        } else {
            game.wrappers.join(" ")
        }
    );
    cli_println!(
        "  Args     : {}",
        if game.args.is_empty() {
            "(none)".to_owned()
        } else {
            game.args.join(" ")
        }
    );
    let own_env = entry.map(|e| &e.env);
    if game.env.is_empty() {
        cli_println!("  Env      : (none)");
    } else {
        cli_println!("  Env      :");
        for (key, value) in &game.env {
            let from = if own_env.is_some_and(|e| e.contains_key(key)) {
                ""
            } else {
                "  (profile)"
            };
            cli_println!("    {key}={value}{from}");
        }
    }
    cli_println!(
        "  Shaders  : {}",
        set_or(
            entry
                .and_then(|e| e.shader_cache.as_ref())
                .map(|c| c.as_text()),
            match &game.shader_cache {
                Some(dir) => format!("resolved: {}", dir.display()),
                None => "resolved: off".into(),
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
