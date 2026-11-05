use crate::core::models::{OwnedGame, SessionState};
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

/// The `config.toml` settings the Steam session, installs and launches read,
/// with defaults filled in and paths expanded. Built by
/// [`load_launcher_config`]; changed through [`crate::ops::config::set_global`].
#[derive(Debug, Clone, PartialEq)]
pub struct LauncherConfig {
    pub steam_library_path: String,
    /// The Proton build for a Windows launch whose profile names none
    /// (`default_proton`).
    pub proton_version: String,
    pub chat_presence: ChatPresence,
    pub enable_cloud_sync: bool,
    /// Network proxy for all HTTP(S) communication. Applied process-wide at
    /// startup; see [`crate::core::net`].
    pub proxy: ProxyConfig,
    /// Whether the daemon resumes interrupted installs on start.
    pub install_resume: crate::library::backlog::InstallResume,
    /// Opt-in gate for experimental features: the browser/OpenID identity
    /// check (`login --openid`) and web-token auth (`login --web-token`).
    pub experimental: bool,
    /// Keep `session.json` encrypted with the OS-keyring key. Enabled
    /// automatically by the first successful login.
    pub encrypt_session: bool,
    /// Whether the OS keyring may be used for the session key. Off for
    /// headless machines and CI, where no keyring service runs.
    pub use_keyring: bool,
    /// Lifetime of a cached `info` record in seconds; `None` is the default
    /// (6h) and `0` disables the cache.
    pub info_cache_ttl: Option<u64>,
}

/// The Proton build used when neither the profile nor `default_proton` names
/// one.
pub const DEFAULT_PROTON: &str = "experimental";

impl LauncherConfig {
    pub async fn load() -> Result<Self> {
        load_launcher_config().await
    }

    /// `config`'s settings, with `~/` expanded against `home` and the Steam
    /// library detected when unset.
    pub fn from_config(config: &crate::config::Config, home: Option<&Path>) -> Self {
        let steam_library_path = match (&config.steam_library, home) {
            (Some(path), Some(home)) => crate::config::expand_tilde(path, home),
            (Some(path), None) => path.clone(),
            (None, _) => detect_steam_path().unwrap_or_else(|| {
                home.map(|home| home.join("Games").join("gaki"))
                    .unwrap_or_else(|| PathBuf::from("~/Games/gaki"))
            }),
        }
        .to_string_lossy()
        .into_owned();
        Self {
            steam_library_path,
            proton_version: config
                .default_proton
                .clone()
                .unwrap_or_else(|| DEFAULT_PROTON.to_owned()),
            chat_presence: config.presence,
            enable_cloud_sync: config.cloud_sync,
            proxy: ProxyConfig {
                url: config.proxy.clone(),
                no_proxy: config.no_proxy.clone(),
            },
            install_resume: config.install_resume,
            experimental: config.experimental,
            encrypt_session: config.encrypt_session,
            use_keyring: config.keyring,
            info_cache_ttl: config.info_cache_ttl,
        }
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self::from_config(
            &crate::config::Config::default(),
            home_dir().ok().as_deref(),
        )
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

/// Load the settings from `config.toml`.
///
/// A *missing* config is not an error: it yields defaults with the Steam library
/// auto-detected. So an `Err` here always means the file exists but could not be read
/// or parsed — never "you have no config yet". Callers must therefore propagate it
/// rather than falling back to defaults, which would silently discard every setting
/// the user has.
pub async fn load_launcher_config() -> Result<LauncherConfig> {
    let config = read_config_toml()?;
    Ok(LauncherConfig::from_config(
        &config,
        home_dir().ok().as_deref(),
    ))
}

/// Parse `config.toml`; a missing file is all defaults.
fn read_config_toml() -> Result<crate::config::Config> {
    let path = config_dir()?.join(crate::config::CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
    };
    toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
}

/// [`read_config_toml`] for the synchronous readers: a broken file is logged
/// and treated as all defaults.
fn read_config_toml_lenient() -> crate::config::Config {
    read_config_toml()
        .inspect_err(|e| tracing::warn!("ignoring config.toml: {e:#}"))
        .unwrap_or_default()
}

/// Synchronously read just the proxy settings. Used at process startup —
/// before the async runtime and worker threads exist — to install the proxy
/// environment variables (see [`crate::core::net::install_proxy_env`]).
/// Returns the default (direct connection) on a missing or unreadable config.
pub fn load_proxy_config_blocking() -> ProxyConfig {
    let config = read_config_toml_lenient();
    ProxyConfig {
        url: config.proxy,
        no_proxy: config.no_proxy,
    }
}

/// The settings read from synchronous code paths (the keyring gate, the info
/// cache TTL). A missing or unreadable `config.toml` yields the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSettings {
    pub use_keyring: bool,
    pub info_cache_ttl: Option<u64>,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self::from(&crate::config::Config::default())
    }
}

impl From<&crate::config::Config> for RuntimeSettings {
    fn from(config: &crate::config::Config) -> Self {
        Self {
            use_keyring: config.keyring,
            info_cache_ttl: config.info_cache_ttl,
        }
    }
}

/// Synchronously read the [`RuntimeSettings`] from `config.toml`.
pub fn load_runtime_settings_blocking() -> RuntimeSettings {
    RuntimeSettings::from(&read_config_toml_lenient())
}

/// `pins.toml`: the games gaki's update commands leave alone, each with the
/// depot manifests it is held at. Written by gaki.
const PINS_FILE: &str = "pins.toml";

/// Every pinned game: app id → depot → manifest id.
pub type Pins = std::collections::BTreeMap<u32, HashMap<u32, u64>>;

fn pins_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(PINS_FILE))
}

/// Read `pins.toml`; a missing file has no pins.
pub fn load_pins() -> Result<Pins> {
    let path = pins_path()?;
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_pins(&text).with_context(|| format!("invalid {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Pins::new()),
        Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
    }
}

fn save_pins(pins: &Pins) -> Result<()> {
    crate::config::edit::write_atomic(&pins_path()?, &render_pins(pins))
}

/// Parse `pins.toml` text. App and depot ids are table keys; manifest ids
/// are strings, since they can exceed TOML's integer range.
fn parse_pins(text: &str) -> Result<Pins> {
    use std::collections::BTreeMap;
    let raw: BTreeMap<String, BTreeMap<String, String>> = toml::from_str(text)?;
    let id = |text: &str, what: &str| -> Result<u64> {
        text.parse()
            .with_context(|| format!("invalid {what} `{text}`"))
    };
    let small = |text: &str, what: &str| -> Result<u32> {
        u32::try_from(id(text, what)?).with_context(|| format!("{what} `{text}` is too large"))
    };
    raw.into_iter()
        .map(|(app, depots)| {
            let depots = depots
                .into_iter()
                .map(|(depot, manifest)| {
                    Ok((small(&depot, "depot id")?, id(&manifest, "manifest id")?))
                })
                .collect::<Result<_>>()?;
            Ok((small(&app, "app id")?, depots))
        })
        .collect()
}

/// `pins` as `pins.toml` text, sorted.
fn render_pins(pins: &Pins) -> String {
    let mut text = String::from(
        "# Written by gaki: games its update commands leave alone, held at these\n\
         # depot manifests (depot id = \"manifest id\").\n",
    );
    for (app_id, depots) in pins {
        text.push_str(&format!("\n[{app_id}]\n"));
        let mut depots: Vec<_> = depots.iter().collect();
        depots.sort();
        for (depot, manifest) in depots {
            text.push_str(&format!("{depot} = \"{manifest}\"\n"));
        }
    }
    text
}

/// Pin a game to `manifests` (depot → manifest ids), locking gaki's update
/// commands for it.
pub async fn set_game_pin(app_id: u32, manifests: HashMap<u32, u64>) -> Result<()> {
    let mut pins = load_pins()?;
    pins.insert(app_id, manifests);
    save_pins(&pins)
}

/// Clear a game's version pin (unlock gaki's update commands). No-op if the
/// game was never pinned.
pub async fn clear_game_pin(app_id: u32) -> Result<()> {
    let mut pins = load_pins()?;
    if pins.remove(&app_id).is_some() {
        save_pins(&pins)?;
    }
    Ok(())
}

/// Return `(pinned, pinned_manifests)` for a game. `(false, empty)` when the
/// game has no pin (or `pins.toml` can't be read).
pub async fn game_pin_state(app_id: u32) -> (bool, HashMap<u32, u64>) {
    match load_pins().map(|mut pins| pins.remove(&app_id)) {
        Ok(Some(manifests)) => (true, manifests),
        Ok(None) => (false, HashMap::new()),
        Err(e) => {
            tracing::warn!("{e:#}");
            (false, HashMap::new())
        }
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

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "config_tests_runtime.rs"]
mod runtime_settings_tests;
