use crate::core::models::{OwnedGame, SessionState, UserConfigStore};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Serialize `value` as pretty JSON and write it to `path`, creating the parent
/// directory first. Shared by the `save_*` helpers so the create-dir / serialize /
/// write / error-context sequence lives in one place.
async fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(value)?;
    fs::write(path, body)
        .await
        .with_context(|| format!("failed writing {}", path.display()))?;
    Ok(())
}

/// Read `path` and parse it as JSON into `T`. Shared by the `load_*` helpers so the
/// read / parse / error-context sequence lives in one place. Callers handle the
/// missing-file fallback themselves, since each returns a different default.
async fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed reading {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed parsing {}", path.display()))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GameConfig {
    pub forced_proton_version: Option<String>,
    pub platform_preference: Option<String>,
    /// Depot → manifest ids this game is pinned to (recorded when the game is
    /// downgraded or pinned). Used to display what an update lock is holding the
    /// game at; empty when the game isn't pinned.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub pinned_manifests: HashMap<u32, u64>,
    /// Whether gaki's update commands are locked for this game. A pinned game is
    /// not upgraded by `update` / `check-updates` (which report it as pinned).
    #[serde(default)]
    pub pinned: bool,
}

/// Network proxy settings applied to all of gaki's HTTP(S) traffic: the Steam
/// Community/store/market web endpoints, depot content downloads (steam-cdn), and the
/// GitHub/Codeberg release lookups used by the Proton manager. Empty by default
/// (a direct connection). See [`crate::core::net`] for how these are applied.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProxyConfig {
    /// Proxy URL for all HTTP(S) requests, e.g. `http://host:8080`,
    /// `http://user:pass@host:8080`, or `socks5://host:1080`. `None` = direct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Comma-separated hosts/domains that bypass the proxy (`NO_PROXY` semantics),
    /// e.g. `localhost,127.0.0.1,.internal`. `None` = no exceptions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<String>,
}

/// Steam presence the session daemon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChatPresence {
    #[default]
    Offline,
    Online,
}

impl ChatPresence {
    /// The raw EPersonaState
    pub fn persona_state(self) -> u32 {
        match self {
            ChatPresence::Offline => 7,
            ChatPresence::Online => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub steam_library_path: String,
    pub proton_version: String,
    // (default: offline).
    #[serde(default)]
    pub chat_presence: ChatPresence,
    pub enable_cloud_sync: bool,
    #[serde(default)]
    pub preferred_launch_options: HashMap<u32, String>,
    #[serde(default)]
    pub game_configs: HashMap<u32, GameConfig>,
    /// Network proxy for all HTTP(S) communication. Empty by default (a direct
    /// connection). Applied process-wide at startup; see [`crate::core::net`].
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// Whether the daemon resumes interrupted installs on start.
    #[serde(default)]
    pub install_resume: crate::library::backlog::InstallResume,
    /// Opt-in gate for experimental features. Off by default. Currently gates the
    /// browser/OpenID identity check (`login --openid`) and web-token auth
    /// (`login --web-token`), which prove identity / enable web-surface commands
    /// but cannot create the full client session that library/install/launch need.
    /// Enable with `gaki-cli config experimental true`.
    #[serde(default)]
    pub experimental: bool,
    /// Keep `session.json` encrypted with the OS-keyring key.
    /// Enabled automatically by the first successful login.
    #[serde(default)]
    pub encrypt_session: bool,
    /// Whether the OS keyring may be used for the session key. Off for
    /// headless machines and CI, where no keyring service runs.
    #[serde(default = "default_true")]
    pub use_keyring: bool,
    /// Lifetime of a cached `info` record in seconds; `None` is the default
    /// (6h) and `0` disables the cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info_cache_ttl: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl LauncherConfig {
    pub async fn load() -> Result<Self> {
        load_launcher_config().await
    }

    pub async fn save(&self) -> Result<()> {
        save_launcher_config(self).await
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        let steam_library_path = detect_steam_path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                home_dir()
                    .map(|home| home.join("Games").join("gaki"))
                    .unwrap_or_else(|_| PathBuf::from("~/Games/gaki"))
                    .to_string_lossy()
                    .into_owned()
            });

        Self {
            steam_library_path,
            proton_version: "experimental".to_string(),
            chat_presence: ChatPresence::default(),
            enable_cloud_sync: true,
            preferred_launch_options: HashMap::new(),
            game_configs: HashMap::new(),
            proxy: ProxyConfig::default(),
            install_resume: crate::library::backlog::InstallResume::default(),
            experimental: false,
            encrypt_session: false,
            use_keyring: true,
            info_cache_ttl: None,
        }
    }
}

/// Whether experimental features are enabled (the `experimental` setting).
pub async fn experimental_enabled() -> bool {
    load_launcher_config()
        .await
        .map(|config| config.experimental)
        .unwrap_or(false)
}

pub fn detect_steam_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let candidates = [
        PathBuf::from(&home).join(".steam/steam"),
        PathBuf::from(&home).join(".local/share/Steam"),
        PathBuf::from(&home).join(".steam/root"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

/// Resolve the user's home directory from `HOME`.
pub fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .context("could not determine home directory (HOME is not set)")
}

/// A config directory set for this process (see [`set_config_dir`]).
static CONFIG_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Use `dir` as the config directory for the rest of the process, instead of
/// `$XDG_CONFIG_HOME/gaki`. Call once, before anything reads the config; a later
/// call is ignored and returns `false`.
pub fn set_config_dir(dir: PathBuf) -> bool {
    CONFIG_DIR_OVERRIDE.set(dir).is_ok()
}

/// The directory set with [`set_config_dir`], if any.
pub fn config_dir_override() -> Option<&'static Path> {
    CONFIG_DIR_OVERRIDE.get().map(PathBuf::as_path)
}

/// gaki's config directory: the one set with [`set_config_dir`], else
/// `$XDG_CONFIG_HOME/gaki`, else `~/.config/gaki`.
pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = CONFIG_DIR_OVERRIDE.get() {
        return Ok(dir.clone());
    }
    // Unit tests never touch the real config dir: they share one throwaway
    // directory per test process (under the system temp dir).
    #[cfg(test)]
    {
        static TEST_DIR: std::sync::LazyLock<PathBuf> =
            std::sync::LazyLock::new(|| tempfile::tempdir().expect("test config dir").keep());
        Ok(TEST_DIR.clone())
    }
    #[cfg(not(test))]
    xdg_dir(std::env::var_os("XDG_CONFIG_HOME"), home_dir, ".config")
}

pub async fn ensure_config_dirs() -> Result<()> {
    let config = config_dir()?;
    fs::create_dir_all(&config).await?;
    let images = image_cache_dir()?;
    fs::create_dir_all(&images).await?;
    Ok(())
}

pub fn image_cache_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("images"))
}

pub fn data_dir() -> Result<PathBuf> {
    config_dir()
}

/// gaki's cache directory: `$XDG_CACHE_HOME/gaki`, or `~/.cache/gaki`.
/// Holds data that can be fetched again, such as cached art and listings.
pub fn cache_dir() -> Result<PathBuf> {
    xdg_dir(std::env::var_os("XDG_CACHE_HOME"), home_dir, ".cache")
}

/// gaki's state directory: `$XDG_STATE_HOME/gaki`, or `~/.local/state/gaki`.
/// Holds data worth keeping between runs that isn't config, such as logs.
pub fn state_dir() -> Result<PathBuf> {
    xdg_dir(std::env::var_os("XDG_STATE_HOME"), home_dir, ".local/state")
}

/// `<base>/gaki` for one XDG base directory: `value` (the variable's value)
/// when set, else `fallback` under the home directory. The XDG spec says a
/// relative value is invalid and must be ignored, the same as an unset one.
fn xdg_dir(
    value: Option<std::ffi::OsString>,
    home: impl FnOnce() -> Result<PathBuf>,
    fallback: &str,
) -> Result<PathBuf> {
    let base = match value.map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => home()?.join(fallback),
    };
    Ok(base.join("gaki"))
}

/// Stored session (`session.json`) path.
pub fn session_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("session.json"))
}

pub async fn load_session() -> Result<SessionState> {
    let session_path = session_path()?;
    if !session_path.exists() {
        return Ok(SessionState::default());
    }
    let raw = fs::read_to_string(&session_path)
        .await
        .with_context(|| format!("failed reading {}", session_path.display()))?;
    // Encrypted sessions decrypt on the fly.
    if let Some(envelope) = crate::core::session_crypto::EncryptedSession::from_json(&raw) {
        if !envelope.keyring {
            // Passwords are gone; a login re-keys it.
            anyhow::bail!(
                "session.json was encrypted with a legacy session password — \
                 run `gaki-cli login` again to re-key it to the OS keyring"
            );
        }
        // Keyring-keyed: automatic, never prompts.
        let key = tokio::task::spawn_blocking(crate::core::session_crypto::keyring_secret)
            .await
            .ok()
            .flatten()
            .context(
                "session.json is keyed to the OS keyring, which is unavailable — \
                 run `gaki-cli login` again",
            )?;
        let plain = crate::core::session_crypto::decrypt(&envelope, &key)?;
        return serde_json::from_slice(&plain)
            .with_context(|| format!("failed parsing decrypted {}", session_path.display()));
    }
    serde_json::from_str(&raw).with_context(|| format!("failed parsing {}", session_path.display()))
}

/// Whether saves must encrypt the session.
///
/// True once a login enabled encryption, or when the file on disk is
/// already an encrypted envelope — an existing encrypted session is never
/// silently rewritten as plaintext.
async fn session_encryption_active(session_path: &Path) -> bool {
    if let Ok(config) = load_launcher_config().await
        && config.encrypt_session
    {
        return true;
    }
    match fs::read_to_string(session_path).await {
        Ok(raw) => crate::core::session_crypto::EncryptedSession::from_json(&raw).is_some(),
        Err(_) => false,
    }
}

pub async fn save_session(session: &SessionState) -> Result<()> {
    let session_path = session_path()?;
    if session_encryption_active(&session_path).await {
        // Automatic OS-keyring key; created on demand, never prompts.
        let key =
            tokio::task::spawn_blocking(crate::core::session_crypto::keyring_secret_or_create)
                .await
                .ok()
                .flatten();
        match key {
            Some(key) => {
                let plain = serde_json::to_vec(session)?;
                let mut envelope = crate::core::session_crypto::encrypt(&plain, &key)?;
                envelope.keyring = true;
                // Display hint, readable without the key.
                envelope.display_name = session
                    .persona_name
                    .clone()
                    .or_else(|| session.account_name.clone());
                write_json_pretty(&session_path, &envelope).await?;
            }
            None => {
                tracing::warn!(
                    "no OS keyring available; session.json stays plaintext \
                     (owner-only permissions)"
                );
                write_json_pretty(&session_path, session).await?;
            }
        }
    } else {
        write_json_pretty(&session_path, session).await?;
    }

    // The session holds a long-lived Steam refresh token; keep it owner-only so it
    // is not world-readable on shared Unix hosts.
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&session_path, std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| format!("failed securing {}", session_path.display()))?;
    }

    Ok(())
}

pub async fn delete_session() -> Result<()> {
    let session_path = session_path()?;
    if session_path.exists() {
        fs::remove_file(session_path).await?;
    }
    Ok(())
}

/// Load `config.json`.
///
/// A *missing* config is not an error: it yields defaults with the Steam library
/// auto-detected. So an `Err` here always means the file exists but could not be read
/// or parsed — never "you have no config yet". Callers must therefore propagate it
/// rather than falling back to defaults: doing so would silently discard every setting
/// the user has, and any caller that then saves would write those defaults straight
/// over the file it failed to read.
pub async fn load_launcher_config() -> Result<LauncherConfig> {
    let path = config_dir()?.join("config.json");
    let mut config = if path.exists() {
        read_json(&path).await.with_context(|| {
            format!(
                "invalid launcher config — fix the JSON in {}, or delete it to regenerate defaults",
                path.display()
            )
        })?
    } else {
        let mut config = LauncherConfig::default();
        if let Some(detected) = detect_steam_path() {
            config.steam_library_path = detected.to_string_lossy().to_string();
        }
        config
    };
    if let Some(toml) = TomlOverlay::load()? {
        toml.apply(&mut config);
    }
    Ok(config)
}

/// The keys set in `config.toml`, laid over the `config.json` values they
/// replace, so every reader of [`LauncherConfig`] sees the TOML settings.
/// Only keys present in the file apply. Goes away with `config.json` (5.8).
struct TomlOverlay {
    keys: toml::Table,
    config: crate::config::Config,
    home: Option<PathBuf>,
}

impl TomlOverlay {
    /// `None` when there is no `config.toml`.
    fn load() -> Result<Option<Self>> {
        let path = config_dir()?.join(crate::config::CONFIG_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
        };
        let parse = || -> Result<Self> {
            Ok(Self {
                keys: toml::from_str(&text)?,
                config: toml::from_str(&text)?,
                home: home_dir().ok(),
            })
        };
        parse()
            .with_context(|| format!("invalid {}", path.display()))
            .map(Some)
    }

    fn has(&self, key: &str) -> bool {
        self.keys.contains_key(key)
    }

    fn path_string(&self, path: &Path) -> String {
        match &self.home {
            Some(home) => crate::config::expand_tilde(path, home),
            None => path.to_path_buf(),
        }
        .to_string_lossy()
        .into_owned()
    }

    fn apply(&self, config: &mut LauncherConfig) {
        let toml = &self.config;
        if let Some(library) = &toml.steam_library {
            config.steam_library_path = self.path_string(library);
        }
        if self.has("presence") {
            config.chat_presence = toml.presence;
        }
        if self.has("cloud_sync") {
            config.enable_cloud_sync = toml.cloud_sync;
        }
        if self.has("proxy") {
            config.proxy.url.clone_from(&toml.proxy);
        }
        if self.has("no_proxy") {
            config.proxy.no_proxy.clone_from(&toml.no_proxy);
        }
        if self.has("install_resume") {
            config.install_resume = toml.install_resume;
        }
        if self.has("experimental") {
            config.experimental = toml.experimental;
        }
        if toml.info_cache_ttl.is_some() {
            config.info_cache_ttl = toml.info_cache_ttl;
        }
    }

    /// Like [`Self::load`], for the synchronous readers: a broken file is
    /// logged and ignored.
    fn load_lenient() -> Option<Self> {
        Self::load()
            .inspect_err(|e| tracing::warn!("ignoring config.toml: {e:#}"))
            .ok()
            .flatten()
    }
}

pub async fn save_launcher_config(config: &LauncherConfig) -> Result<()> {
    let path = config_dir()?.join("config.json");
    write_json_pretty(&path, config).await
}

/// Synchronously read just the proxy settings from `config.json`. Used at process
/// startup — before the async runtime and worker threads exist — to install the proxy
/// environment variables (see [`crate::core::net::install_proxy_env`]), which is why it
/// can't use the async loader. Returns the default (direct connection) on a missing,
/// unreadable, or unparseable config.
pub fn load_proxy_config_blocking() -> ProxyConfig {
    let Ok(path) = config_dir().map(|dir| dir.join("config.json")) else {
        return ProxyConfig::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return ProxyConfig::default();
    };
    let mut proxy = serde_json::from_str::<LauncherConfig>(&raw)
        .map(|config| config.proxy)
        .unwrap_or_default();
    if let Some(toml) = TomlOverlay::load_lenient() {
        if toml.has("proxy") {
            proxy.url.clone_from(&toml.config.proxy);
        }
        if toml.has("no_proxy") {
            proxy.no_proxy.clone_from(&toml.config.no_proxy);
        }
    }
    proxy
}

/// The settings read from synchronous code paths (the keyring gate, the info
/// cache TTL). Parsed leniently from `config.json`: other
/// fields are ignored, and a missing or unreadable file yields the defaults.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RuntimeSettings {
    pub use_keyring: bool,
    pub info_cache_ttl: Option<u64>,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            use_keyring: true,
            info_cache_ttl: None,
        }
    }
}

/// Synchronously read the [`RuntimeSettings`] from `config.json`.
pub fn load_runtime_settings_blocking() -> RuntimeSettings {
    let mut settings: RuntimeSettings = config_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("config.json")).ok())
        .map(|raw| parse_runtime_settings(&raw))
        .unwrap_or_default();
    if let Some(toml) = TomlOverlay::load_lenient()
        && toml.config.info_cache_ttl.is_some()
    {
        settings.info_cache_ttl = toml.config.info_cache_ttl;
    }
    settings
}

fn parse_runtime_settings(raw: &str) -> RuntimeSettings {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Pin a game to `manifests` (depot → manifest ids), locking gaki's update
/// commands for it. Preserves the game's other `GameConfig` fields.
pub async fn set_game_pin(app_id: u32, manifests: HashMap<u32, u64>) -> Result<()> {
    let mut cfg = load_launcher_config().await?;
    let gc = cfg.game_configs.entry(app_id).or_default();
    gc.pinned = true;
    gc.pinned_manifests = manifests;
    save_launcher_config(&cfg).await
}

/// Clear a game's version pin (unlock gaki's update commands). No-op if the
/// game was never pinned.
pub async fn clear_game_pin(app_id: u32) -> Result<()> {
    let mut cfg = load_launcher_config().await?;
    if let Some(gc) = cfg.game_configs.get_mut(&app_id) {
        gc.pinned = false;
        gc.pinned_manifests.clear();
    }
    save_launcher_config(&cfg).await
}

/// Return `(pinned, pinned_manifests)` for a game, reading the launcher config.
/// `(false, empty)` when the game has no pin (or the config can't be read).
pub async fn game_pin_state(app_id: u32) -> (bool, HashMap<u32, u64>) {
    match load_launcher_config().await {
        Ok(cfg) => cfg
            .game_configs
            .get(&app_id)
            .map(|gc| (gc.pinned, gc.pinned_manifests.clone()))
            .unwrap_or((false, HashMap::new())),
        Err(_) => (false, HashMap::new()),
    }
}

pub fn library_cache_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("library_cache.json"))
}

pub async fn save_library_cache(owned_games: &[OwnedGame]) -> Result<()> {
    write_json_pretty(&library_cache_path()?, &owned_games).await
}

pub async fn load_library_cache() -> Result<Vec<OwnedGame>> {
    let path = library_cache_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path).await
}

/// Default lifetime of a cached `gaki-cli info` record before it is re-fetched
/// from Steam. Store metadata is effectively static for hours, while drivers like
/// Heroic call `info` repeatedly; caching it collapses those into one Steam CM
/// logon + StoreBrowse/PICS round-trip per app per window. The `info_cache_ttl`
/// setting overrides it (in seconds); `0` disables the cache.
pub const INFO_CACHE_DEFAULT_TTL_SECS: u64 = 6 * 60 * 60;

/// The configured `info` cache TTL.
pub fn info_cache_ttl() -> std::time::Duration {
    let secs = load_runtime_settings_blocking()
        .info_cache_ttl
        .unwrap_or(INFO_CACHE_DEFAULT_TTL_SECS);
    std::time::Duration::from_secs(secs)
}

/// A cached `gaki-cli info` result: the StoreBrowse metadata plus resolved DLC
/// `(app_id, name)` pairs, stamped with the unix time it was fetched so a reader
/// can honour a TTL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAppInfo {
    pub fetched_at: u64,
    pub details: crate::steam_client::StoreAppInfo,
    pub dlc: Vec<(u32, Option<String>)>,
}

fn info_cache_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("info_cache"))
}

/// Per-app cache file, one per `(app id, country)` so concurrent invocations
/// for different apps never clobber each other. The country is sanitized to
/// keep it safe as a filename component.
fn info_cache_path(app_id: u32, country: &str) -> Result<PathBuf> {
    let cc: String = country
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    Ok(info_cache_dir()?.join(format!("{app_id}.{cc}.json")))
}

use crate::core::utils::now_unix;

/// Load a cached `info` record for `app_id` if one exists and is still within
/// `ttl`. Returns `None` on a miss, a stale entry, or any read/parse error, so the
/// caller falls through to a fresh fetch. A zero `ttl` always misses.
pub async fn load_info_cache(
    app_id: u32,
    country: &str,
    ttl: std::time::Duration,
) -> Option<CachedAppInfo> {
    if ttl.is_zero() {
        return None;
    }
    let path = info_cache_path(app_id, country).ok()?;
    let raw = fs::read_to_string(&path).await.ok()?;
    let cached: CachedAppInfo = serde_json::from_str(&raw).ok()?;
    let age = now_unix().saturating_sub(cached.fetched_at);
    (age <= ttl.as_secs()).then_some(cached)
}

/// Persist an `info` record for `app_id`, stamped with the current time.
pub async fn save_info_cache(
    app_id: u32,
    country: &str,
    details: &crate::steam_client::StoreAppInfo,
    dlc: &[(u32, Option<String>)],
) -> Result<()> {
    let record = CachedAppInfo {
        fetched_at: now_unix(),
        details: details.clone(),
        dlc: dlc.to_vec(),
    };
    write_json_pretty(&info_cache_path(app_id, country)?, &record).await
}

/// Cache file for a store name table (`tag_names`, `category_names`).
fn name_cache_path(kind: &str) -> Result<PathBuf> {
    Ok(data_dir()?.join(format!("{kind}.json")))
}

/// Cached store names of `kind`; empty on miss.
pub async fn load_name_cache(kind: &str) -> std::collections::HashMap<u32, String> {
    let Ok(path) = name_cache_path(kind) else {
        return Default::default();
    };
    read_json(&path).await.unwrap_or_default()
}

/// Persist store names of `kind`.
pub async fn save_name_cache(
    kind: &str,
    names: &std::collections::HashMap<u32, String>,
) -> Result<()> {
    write_json_pretty(&name_cache_path(kind)?, names).await
}

/// Load the per-game user config store (`user_apps.json`).
///
/// A *missing* file is not an error — it yields an empty store. So an `Err` here always
/// means the file exists but could not be parsed, and callers must propagate it rather
/// than falling back to an empty store: silently dropping every per-game setting (launch
/// options, env vars, the Steam-runtime policy) on one typo is exactly the footgun this
/// avoids. See the same contract on [`load_launcher_config`].
pub async fn load_user_configs() -> Result<UserConfigStore> {
    let path = config_dir()?.join("user_apps.json");
    if !path.exists() {
        return Ok(UserConfigStore::new());
    }
    read_json(&path).await.with_context(|| {
        format!(
            "invalid per-game config — fix the JSON in {}, or delete it to start fresh",
            path.display()
        )
    })
}

pub async fn save_user_configs(configs: &UserConfigStore) -> Result<()> {
    let path = config_dir()?.join("user_apps.json");
    write_json_pretty(&path, configs).await
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "config_tests_runtime.rs"]
mod runtime_settings_tests;
