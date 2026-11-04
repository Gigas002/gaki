//! Reading and changing the launcher configuration.
//!
//! Phase 5 replaces this JSON model with TOML config/profiles/games files;
//! these operations keep the current behaviour until then.

use crate::core::config::{
    GameConfig, LauncherConfig, ProxyConfig, load_launcher_config, load_user_configs,
    save_launcher_config, save_user_configs,
};
use crate::core::models::DepotPlatform;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// A requested change to one optional setting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Change<T> {
    /// Leave it as it is.
    #[default]
    Keep,
    /// Reset it to the default.
    Clear,
    /// Set it to this value.
    Set(T),
}

impl<T> Change<T> {
    /// Apply to `slot`; returns whether anything was requested.
    fn apply_to(self, slot: &mut Option<T>) -> bool {
        match self {
            Change::Keep => false,
            Change::Clear => {
                *slot = None;
                true
            }
            Change::Set(v) => {
                *slot = Some(v);
                true
            }
        }
    }
}

/// Load the launcher configuration (defaults when the file is missing).
pub async fn load() -> Result<LauncherConfig> {
    load_launcher_config().await
}

/// Load, apply `edit`, and save the launcher configuration. Returns the saved
/// configuration. Nothing is written if `edit` fails.
pub async fn update(
    edit: impl FnOnce(&mut LauncherConfig) -> Result<()>,
) -> Result<LauncherConfig> {
    let mut config = load_launcher_config().await?;
    edit(&mut config)?;
    save_launcher_config(&config).await?;
    Ok(config)
}

/// The experimental-features gate.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ExperimentalStatus {
    /// The persisted setting.
    pub experimental: bool,
    /// What actually applies (the setting; kept for drivers that read it).
    pub effective: bool,
}

/// Read, and with `set` change, the experimental-features gate.
pub async fn experimental(set: Option<bool>) -> Result<ExperimentalStatus> {
    let config = match set {
        Some(value) => {
            update(|c| {
                c.experimental = value;
                Ok(())
            })
            .await?
        }
        None => load().await?,
    };
    Ok(ExperimentalStatus {
        experimental: config.experimental,
        effective: config.experimental,
    })
}

/// A change to the network proxy setting.
#[derive(Debug, Clone, Default)]
pub struct ProxyChange {
    /// Drop both the URL and the bypass list (wins over the other fields).
    pub clear: bool,
    /// New proxy URL; an empty string means a direct connection.
    pub url: Option<String>,
    /// New bypass list; an empty string means no exceptions.
    pub no_proxy: Option<String>,
}

impl ProxyChange {
    /// Whether this changes anything.
    pub fn is_noop(&self) -> bool {
        !self.clear && self.url.is_none() && self.no_proxy.is_none()
    }
}

/// Apply `change` to the proxy setting (validating the URL) and return the
/// resulting setting. A no-op change only reads.
pub async fn update_proxy(change: ProxyChange) -> Result<ProxyConfig> {
    if change.is_noop() {
        return Ok(load().await?.proxy);
    }
    let config = update(|config| {
        if change.clear {
            config.proxy = ProxyConfig::default();
            return Ok(());
        }
        if let Some(url) = change.url {
            let value = url.trim();
            if value.is_empty() {
                config.proxy.url = None;
            } else {
                crate::core::net::validate_proxy_url(value)?;
                config.proxy.url = Some(value.to_string());
            }
        }
        if let Some(no_proxy) = change.no_proxy {
            let value = no_proxy.trim();
            config.proxy.no_proxy = (!value.is_empty()).then(|| value.to_string());
        }
        Ok(())
    })
    .await
    .context("failed saving proxy config")?;
    Ok(config.proxy)
}

/// App ids that have any per-game settings, in either store.
pub async fn games_with_settings() -> Result<BTreeSet<u32>> {
    let config = load().await?;
    let user_configs = load_user_configs().await?;
    Ok(config
        .game_configs
        .keys()
        .chain(user_configs.keys())
        .copied()
        .collect())
}

/// Reset every game's per-game settings (both stores). Returns how many games
/// had settings.
pub async fn clear_all_game_settings() -> Result<usize> {
    let count = games_with_settings().await?.len();
    if count == 0 {
        return Ok(0);
    }
    update(|c| {
        c.game_configs.clear();
        Ok(())
    })
    .await
    .context("failed saving game config")?;
    save_user_configs(&Default::default())
        .await
        .context("failed saving per-game config")?;
    Ok(count)
}

/// A change to one game's launch settings.
#[derive(Debug, Clone, Default)]
pub struct GameSettingsChange {
    /// Reset every per-game setting (both stores); the other fields are ignored.
    pub reset: bool,
    pub proton: Change<String>,
    pub platform: Change<DepotPlatform>,
    pub launch_script: Change<PathBuf>,
}

/// One game's launch settings, plus what they fall back to.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameSettings {
    pub app_id: u32,
    pub forced_proton_version: Option<String>,
    pub platform_preference: Option<String>,
    pub launch_script: Option<String>,
    /// The global default Proton version, used when none is forced.
    #[serde(skip)]
    pub default_proton_version: String,
    /// Which platform payloads are installed, as `(linux, windows)`; `None`
    /// when the game isn't installed or its directory can't be read.
    #[serde(skip)]
    pub installed_platforms: Option<(bool, bool)>,
}

/// Apply `change` to `app_id`'s settings and return them. Also returns whether
/// anything was saved.
pub async fn update_game(app_id: u32, change: GameSettingsChange) -> Result<(GameSettings, bool)> {
    let mut config = load().await?;
    let changed = if change.reset {
        // The launch tweaks (env, options, graphics layers) live in a second store.
        let mut user_configs = load_user_configs().await?;
        if user_configs.remove(&app_id).is_some() {
            save_user_configs(&user_configs)
                .await
                .context("failed saving per-game config")?;
        }
        config.game_configs.remove(&app_id).is_some()
    } else {
        let entry = config.game_configs.entry(app_id).or_default();
        let platform = match change.platform {
            Change::Keep => Change::Keep,
            Change::Clear => Change::Clear,
            Change::Set(p) => Change::Set(platform_name(p).to_string()),
        };
        let script = match change.launch_script {
            Change::Keep => Change::Keep,
            Change::Clear => Change::Clear,
            Change::Set(p) => Change::Set(p.to_string_lossy().into_owned()),
        };
        // Evaluate every change; `|` doesn't short-circuit.
        change.proton.apply_to(&mut entry.forced_proton_version)
            | platform.apply_to(&mut entry.platform_preference)
            | script.apply_to(&mut entry.launch_script)
    };
    if changed {
        save_launcher_config(&config)
            .await
            .context("failed saving game config")?;
    }

    let entry: GameConfig = config
        .game_configs
        .get(&app_id)
        .cloned()
        .unwrap_or_default();
    Ok((
        GameSettings {
            app_id,
            forced_proton_version: entry.forced_proton_version,
            platform_preference: entry.platform_preference,
            launch_script: entry.launch_script,
            default_proton_version: config.proton_version,
            installed_platforms: installed_platforms(app_id).await,
        },
        changed,
    ))
}

/// The config spelling of a platform preference.
pub fn platform_name(platform: DepotPlatform) -> &'static str {
    match platform {
        DepotPlatform::Windows => "windows",
        DepotPlatform::Linux => "linux",
    }
}

/// Which platform payloads `app_id` has installed: `(linux, windows)`.
async fn installed_platforms(app_id: u32) -> Option<(bool, bool)> {
    let installed = crate::library::scan_installed_app_info().await.ok()?;
    let info = installed.get(&app_id)?;
    crate::library::platform::detect_installed_platform_set(&info.install_path.to_string_lossy())
}

#[cfg(test)]
mod tests;
