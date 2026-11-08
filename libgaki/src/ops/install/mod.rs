//! Installing, updating, verifying, moving and removing games; the install
//! backlog; and version pinning (downgrade, pin, unpin).
//!
//! Operations that stream progress take a `drive` callback: it consumes the
//! progress receiver (rendering it however the caller likes) and returns
//! whether the operation reported completion.

use crate::core::config::{clear_game_pin, game_pin_state, load_launcher_config, set_game_pin};
use crate::core::error::{ErrorKind, TypedError, classify};
use crate::core::models::{DepotPlatform, DownloadProgress, DownloadState, LibraryGame};
use crate::library::backlog::{self, BacklogEntry, BacklogState};
use crate::steam_client::{DepotManifestInfo, InstallOptions, InstallSizeEstimate, SteamClient};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tokio::sync::mpsc::Receiver;

/// Shared progress state of one running operation.
pub type SharedDownloadState = Arc<RwLock<DownloadState>>;

/// In-flight operations in this process, by app id, so a later request (e.g.
/// "stop" or "list" arriving over another daemon connection, or another TUI
/// view) can reach a running download's shared state.
static ACTIVE_INSTALLS: OnceLock<Mutex<HashMap<u32, SharedDownloadState>>> = OnceLock::new();

/// The process-wide registry of in-flight operations.
pub fn active_installs() -> &'static Mutex<HashMap<u32, SharedDownloadState>> {
    ACTIVE_INSTALLS.get_or_init(|| Mutex::new(HashMap::new()))
}

mod lock;

/// RAII registration of a running operation: added on construction, removed on
/// drop (success, error, `?` or abort alike). Dropping it also raises the
/// operation's abort flag. It holds the app's cross-process lock too, so no
/// other gaki process starts, resumes or forgets the same install meanwhile.
pub struct InstallGuard {
    app_id: u32,
    state: SharedDownloadState,
    _lock: lock::InstallLock,
}

impl InstallGuard {
    /// Register a fresh shared [`DownloadState`] for `app_id`.
    pub fn begin(app_id: u32) -> Result<(Self, SharedDownloadState)> {
        let state = Arc::new(RwLock::new(DownloadState::default()));
        let guard = Self::register(app_id, Arc::clone(&state))?;
        Ok((guard, state))
    }

    /// Register `state` as `app_id`'s running operation. Fails if one is
    /// already running for that app, here or in another gaki process.
    pub fn register(app_id: u32, state: SharedDownloadState) -> Result<Self> {
        let mut map = active_installs()
            .lock()
            .map_err(|_| anyhow::anyhow!("install registry lock poisoned"))?;
        if map.contains_key(&app_id) {
            bail!("an install/update/verify for app {app_id} is already running");
        }
        let Some(lock) = lock::try_acquire(app_id)? else {
            return Err(running_elsewhere(app_id));
        };
        map.insert(app_id, Arc::clone(&state));
        Ok(Self {
            app_id,
            state,
            _lock: lock,
        })
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if let Ok(state) = self.state.read() {
            state.abort_signal.store(true, Ordering::SeqCst);
        }
        if let Ok(mut map) = active_installs().lock() {
            map.remove(&self.app_id);
        }
    }
}

/// Prefer the Windows depot; Proton runs it on Linux.
///
/// Native Linux builds are often older or fussier than the Windows build under
/// Proton/DXVK, so Windows is the default everywhere; asking for Linux opts
/// into the native build explicitly.
pub fn auto_select_platform(platforms: &[DepotPlatform]) -> DepotPlatform {
    platforms
        .iter()
        .copied()
        .find(|p| *p == DepotPlatform::Windows)
        .or_else(|| platforms.first().copied())
        .unwrap_or(DepotPlatform::Windows)
}

/// Lower-case name of a platform, as recorded in the backlog and JSON.
fn platform_name(platform: DepotPlatform) -> String {
    format!("{platform:?}").to_lowercase()
}

/// The platform to install, and the appinfo fetched to pick it.
struct PlatformChoice {
    platform: DepotPlatform,
    auto_selected: bool,
    cached_vdf: Option<Vec<u8>>,
}

async fn choose_platform(
    client: &mut SteamClient,
    app_id: u32,
    requested: Option<DepotPlatform>,
) -> Result<PlatformChoice> {
    if let Some(platform) = requested {
        return Ok(PlatformChoice {
            platform,
            auto_selected: false,
            cached_vdf: None,
        });
    }
    let (platforms, buffer) = client
        .get_available_platforms(app_id)
        .await
        .context("failed to detect available platforms")?;
    // The configured platform (`games.toml`, the profile, `default_platform`)
    // when the game ships it; a config problem only costs the preference.
    let configured = match crate::settings::resolve_user(app_id, &Default::default()) {
        Ok(settings) => settings.platform.map(DepotPlatform::from),
        Err(e) => {
            tracing::warn!(appid = app_id, "ignoring the configured platform: {e:#}");
            None
        }
    };
    if let Some(platform) = configured.filter(|p| platforms.contains(p)) {
        return Ok(PlatformChoice {
            platform,
            auto_selected: false,
            cached_vdf: Some(buffer),
        });
    }
    Ok(PlatformChoice {
        platform: auto_select_platform(&platforms),
        auto_selected: true,
        cached_vdf: Some(buffer),
    })
}

/// The size of an install, estimated without downloading anything.
#[derive(Debug, Clone)]
pub struct Estimate {
    pub platform: DepotPlatform,
    pub size: InstallSizeEstimate,
}

/// Estimate the download and disk size of installing `app_id`.
pub async fn estimate(
    client: &mut SteamClient,
    app_id: u32,
    platform: Option<DepotPlatform>,
) -> Result<Estimate> {
    let platform = choose_platform(client, app_id, platform).await?.platform;
    let size = client
        .estimate_install_size(app_id, platform)
        .await
        .with_context(|| format!("failed to estimate install size for app {app_id}"))?;
    Ok(Estimate { platform, size })
}

/// One install, fresh or resumed from the backlog.
#[derive(Debug, Clone)]
pub struct InstallRequest {
    pub app_id: u32,
    /// `None` picks one with [`auto_select_platform`].
    pub platform: Option<DepotPlatform>,
    /// Recorded in the backlog so a resume manages Steam the same way.
    pub restart_steam: bool,
    /// Steam library to install into; `None` uses the configured one.
    pub library: Option<String>,
    /// Bytes a resume already has on disk.
    pub on_disk: u64,
    /// Automatic resume attempts so far (0 for a manual run).
    pub auto_attempts: u32,
}

/// A checked install, ready to [`run`](PreparedInstall::run).
pub struct PreparedInstall {
    request: InstallRequest,
    /// The app is a DLC (it installs into its base game, whose appmanifest a
    /// running Steam would overwrite).
    pub is_dlc: bool,
    pub platform: DepotPlatform,
    /// The platform was picked automatically.
    pub auto_selected: bool,
    library_root: String,
    cached_vdf: Option<Vec<u8>>,
}

/// Resolve the platform and library and check free space (`DiskFull`).
/// Touches nothing on disk, so Steam may still be running.
pub async fn prepare(client: &mut SteamClient, request: InstallRequest) -> Result<PreparedInstall> {
    let app_id = request.app_id;
    let is_dlc = client.resolve_dlc_parent(app_id).await.is_some();
    let choice = choose_platform(client, app_id, request.platform).await?;
    let library_root = match &request.library {
        Some(lib) => lib.clone(),
        None => load_launcher_config().await?.steam_library_path,
    };
    // DLC goes into the base game's library and isn't covered by the estimate.
    if !is_dlc {
        let est = client
            .estimate_install_size(app_id, choice.platform)
            .await
            .with_context(|| format!("failed to estimate install size for app {app_id}"))?;
        if let Some(free) = crate::core::utils::available_space_for(Path::new(&library_root)) {
            // A resume's partial files already count.
            check_space(
                &library_root,
                est.disk_size.saturating_sub(request.on_disk),
                free,
            )?;
        }
    }
    Ok(PreparedInstall {
        request,
        is_dlc,
        platform: choice.platform,
        auto_selected: choice.auto_selected,
        library_root,
        cached_vdf: choice.cached_vdf,
    })
}

fn check_space(library: &str, needed: u64, free: u64) -> Result<()> {
    if needed <= free {
        return Ok(());
    }
    Err(TypedError::new(
        ErrorKind::DiskFull,
        format!(
            "not enough space on {library}: needs {} but only {} free",
            crate::core::utils::human_bytes(needed),
            crate::core::utils::human_bytes(free)
        ),
    )
    .into())
}

impl PreparedInstall {
    /// Download the install, keeping the backlog in step: the entry is written
    /// before the download starts (so a crash stays resumable), removed on
    /// success, and marked paused/aborted/interrupted otherwise.
    ///
    /// A DLC edits its base game's appmanifest, so stop Steam first when
    /// [`is_dlc`](Self::is_dlc) is set.
    pub async fn run<Fut: Future<Output = Result<bool>>>(
        self,
        client: &SteamClient,
        drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
    ) -> Result<()> {
        let Self {
            request,
            is_dlc,
            platform,
            library_root,
            cached_vdf,
            ..
        } = self;
        let app_id = request.app_id;
        // Registered so stop/pause/list (possibly from another connection)
        // reach it. Verified chunks count themselves; don't seed.
        let state = Arc::new(RwLock::new(DownloadState::default()));
        let _guard = InstallGuard::register(app_id, Arc::clone(&state))?;

        let started = backlog::now();
        let mut record = BacklogEntry {
            app_id,
            name: String::new(),
            state: BacklogState::Interrupted,
            platform: platform_name(platform),
            library: library_root.clone(),
            install_dir: None,
            is_dlc,
            restart_steam: request.restart_steam,
            downloaded_bytes: request.on_disk,
            total_bytes: 0,
            reason: Some("install did not finish".into()),
            error_type: None,
            auto_attempts: request.auto_attempts,
            added_at: started,
            updated_at: started,
        };
        note_backlog(backlog::upsert(record.clone()));

        let options = InstallOptions {
            cached_vdf,
            library_override: Some(library_root),
            ..Default::default()
        };
        let outcome = match client
            .install_game(app_id, platform, options, Arc::clone(&state))
            .await
        {
            Ok(rx) => {
                // The target exists now; record it so a crash still lets
                // `forget` clean up the partial directory safely.
                if let Ok(s) = state.read() {
                    record.name = s.app_name.clone();
                    record.install_dir = s.install_dir.clone();
                    record.total_bytes = s.total_bytes;
                }
                note_backlog(backlog::upsert(record.clone()));
                match drive(rx).await {
                    Ok(true) => Ok(()),
                    Ok(false) if stop_requested(&state) => {
                        Err(TypedError::new(ErrorKind::Cancelled, "download stopped").into())
                    }
                    Ok(false) => Err(anyhow::anyhow!(
                        "install of app {app_id} ended without a result"
                    )),
                    Err(err) => Err(err),
                }
            }
            Err(err) => {
                Err(err).with_context(|| format!("failed to start install for app {app_id}"))
            }
        };
        settle_backlog(app_id, record, &state, outcome)
    }
}

/// Log, never fail on, backlog writes.
fn note_backlog<T>(result: Result<T>) {
    if let Err(e) = result {
        tracing::warn!("install backlog not updated: {e:#}");
    }
}

/// Whether pause or stop was asked.
fn stop_requested(state: &SharedDownloadState) -> bool {
    state
        .read()
        .map(|s| s.abort_signal.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// Settle the backlog entry from `outcome`.
fn settle_backlog(
    app_id: u32,
    mut record: BacklogEntry,
    state: &SharedDownloadState,
    outcome: Result<()>,
) -> Result<()> {
    let Err(err) = outcome else {
        note_backlog(backlog::remove(app_id));
        return Ok(());
    };
    let mut paused = false;
    if let Ok(s) = state.read() {
        paused = s.is_paused;
        record.name = s.app_name.clone();
        record.install_dir = s.install_dir.clone();
        record.downloaded_bytes = record.downloaded_bytes.max(s.downloaded_bytes);
        record.total_bytes = s.total_bytes;
    }
    let (state, err) = classify_outcome(app_id, err, paused);
    record.state = state;
    record.updated_at = backlog::now();
    if state == BacklogState::Interrupted {
        record.reason = Some(format!("{err:#}"));
        record.error_type = serde_json::to_value(classify(&err).kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string));
    } else {
        record.reason = None;
        record.error_type = None;
    }
    note_backlog(backlog::upsert(record));
    Err(err)
}

/// The backlog state a failed install lands in, and the error to report.
fn classify_outcome(
    app_id: u32,
    err: anyhow::Error,
    paused: bool,
) -> (BacklogState, anyhow::Error) {
    if classify(&err).kind != ErrorKind::Cancelled {
        return (BacklogState::Interrupted, err);
    }
    if paused {
        let paused = TypedError::new(
            ErrorKind::Cancelled,
            format!("install of app {app_id} paused; it stays in the backlog to resume"),
        );
        (BacklogState::Paused, paused.into())
    } else {
        let stopped = err.context(format!(
            "install of app {app_id} stopped; it stays in the backlog"
        ));
        (BacklogState::Aborted, stopped)
    }
}

/// What [`signal`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signalled {
    /// The running install was asked to stop or pause.
    Running,
    /// Running in another gaki process, which has to stop it.
    Elsewhere,
    /// Not running; its backlog entry was relabelled to this state.
    Relabelled(BacklogState),
    /// Neither running nor in the backlog.
    NotFound,
}

/// Stop (or pause) a running install; a queued one is just relabelled.
pub fn signal(app_id: u32, pause: bool) -> Result<Signalled> {
    let state = active_installs()
        .lock()
        .ok()
        .and_then(|map| map.get(&app_id).cloned());
    let Some(state) = state else {
        if lock::is_held(app_id) {
            return Ok(Signalled::Elsewhere);
        }
        let target = if pause {
            BacklogState::Paused
        } else {
            BacklogState::Aborted
        };
        return Ok(if backlog::set_state(app_id, target)? {
            Signalled::Relabelled(target)
        } else {
            Signalled::NotFound
        });
    };
    // Pause first; the abort reads it.
    if let Ok(mut guard) = state.write() {
        guard.is_paused = pause;
        guard.abort_signal.store(true, Ordering::SeqCst);
    }
    Ok(Signalled::Running)
}

/// Whether an install, update or verify is running for `app_id` in this
/// process.
pub fn is_running(app_id: u32) -> bool {
    active_installs()
        .lock()
        .is_ok_and(|m| m.contains_key(&app_id))
}

/// Whether another gaki process (the session daemon, another TUI) is
/// running an install, update or verify for `app_id`.
pub fn is_running_elsewhere(app_id: u32) -> bool {
    !is_running(app_id) && lock::is_held(app_id)
}

fn running_elsewhere(app_id: u32) -> anyhow::Error {
    TypedError::new(
        ErrorKind::InvalidInput,
        format!(
            "app {app_id} is downloading in another gaki process; stop it there              (`gaki-cli install stop {app_id}` for one gaki-cli started)"
        ),
    )
    .into()
}

/// One install, running or in the backlog.
#[derive(Debug, Clone)]
pub struct InstallRow {
    pub app_id: u32,
    pub name: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    /// Live status text, or the backlog's reason.
    pub status: String,
    pub is_downloading: bool,
    /// `running`, `running elsewhere` (another gaki process), or the
    /// backlog state.
    pub state: &'static str,
    /// The backlog entry, for rows that are not running.
    pub entry: Option<BacklogEntry>,
}

/// [`InstallRow::state`] of a backlog entry another gaki process is running.
pub const RUNNING_ELSEWHERE: &str = "running elsewhere";

/// Running installs (by app id), then the backlog.
pub fn list() -> Result<Vec<InstallRow>> {
    let mut rows: Vec<InstallRow> = Vec::new();
    if let Ok(map) = active_installs().lock() {
        for state in map.values() {
            if let Ok(s) = state.read() {
                rows.push(InstallRow {
                    app_id: s.app_id,
                    name: s.app_name.clone(),
                    total_bytes: s.total_bytes,
                    downloaded_bytes: s.downloaded_bytes,
                    status: s.status_text.clone(),
                    is_downloading: s.is_downloading,
                    state: "running",
                    entry: None,
                });
            }
        }
    }
    rows.sort_by_key(|r| r.app_id);
    // A running install's entry is itself.
    let running: Vec<u32> = rows.iter().map(|r| r.app_id).collect();
    for entry in backlog::load()? {
        if running.contains(&entry.app_id) {
            continue;
        }
        rows.push(InstallRow {
            app_id: entry.app_id,
            name: entry.name.clone(),
            total_bytes: entry.total_bytes,
            downloaded_bytes: entry.downloaded_bytes,
            status: entry.reason.clone().unwrap_or_default(),
            is_downloading: false,
            state: if lock::is_held(entry.app_id) {
                RUNNING_ELSEWHERE
            } else {
                entry.state.as_str()
            },
            entry: Some(entry),
        });
    }
    Ok(rows)
}

/// The backlog entries to resume: one app (`NotFound` if it is not in the
/// backlog), or every entry with `all`.
pub fn resume_targets(app_id: Option<u32>, all: bool) -> Result<Vec<u32>> {
    let entries = backlog::load()?;
    if all {
        return Ok(entries.iter().map(|e| e.app_id).collect());
    }
    let id = app_id.unwrap_or_default();
    if !entries.iter().any(|e| e.app_id == id) {
        return Err(TypedError::new(
            ErrorKind::NotFound,
            format!("app {id} is not in the install backlog"),
        )
        .into());
    }
    Ok(vec![id])
}

/// The install request that resumes `app_id`'s backlog entry, with the entry.
/// `None` when the entry is gone, already running here, or (with `auto`) its
/// attempt count could not be bumped. `auto` counts the attempt; a manual
/// resume resets it. Running in another gaki process is an error.
pub fn resume_request(app_id: u32, auto: bool) -> Result<Option<(InstallRequest, BacklogEntry)>> {
    let Some(entry) = backlog::get(app_id)? else {
        return Ok(None);
    };
    if is_running(app_id) {
        return Ok(None);
    }
    if lock::is_held(app_id) {
        return Err(running_elsewhere(app_id));
    }
    // Counted now, so early failures count.
    let auto_attempts = if auto {
        match backlog::bump_auto_attempts(app_id) {
            Ok(Some(n)) => n,
            Ok(None) => return Ok(None),
            Err(e) => {
                tracing::warn!("install backlog not updated: {e:#}");
                return Ok(None);
            }
        }
    } else {
        0
    };
    let request = InstallRequest {
        app_id,
        // A queued import may have no platform yet: pick it as `install` does.
        platform: match entry.platform.as_str() {
            "linux" => Some(DepotPlatform::Linux),
            "windows" => Some(DepotPlatform::Windows),
            _ => None,
        },
        restart_steam: entry.restart_steam,
        library: Some(entry.library.clone()).filter(|l| !l.is_empty()),
        on_disk: entry.downloaded_bytes,
        auto_attempts,
    };
    Ok(Some((request, entry)))
}

/// Queue a library file's entries in the install backlog, so `install
/// resume` (or a restart) can carry on where an import stopped. `library` is
/// the Steam library to install into, `None` for the configured one. Returns
/// the app ids in file order.
pub async fn queue_import(
    entries: &[crate::library::transfer::LibraryEntry],
    library: Option<String>,
) -> Result<Vec<u32>> {
    let library = match library {
        Some(library) => library,
        None => load_launcher_config().await?.steam_library_path,
    };
    let now = backlog::now();
    backlog::update(|list| queue_in(list, entries, &library, now))?;
    Ok(entries.iter().map(|e| e.app_id).collect())
}

/// Add `entries` to `list` as queued. One already in the backlog keeps its
/// state and partial files.
pub(crate) fn queue_in(
    list: &mut Vec<BacklogEntry>,
    entries: &[crate::library::transfer::LibraryEntry],
    library: &str,
    now: u64,
) {
    for entry in entries {
        if list.iter().any(|e| e.app_id == entry.app_id) {
            continue;
        }
        list.push(BacklogEntry {
            app_id: entry.app_id,
            name: entry.name.clone(),
            state: BacklogState::Queued,
            platform: entry
                .platform
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
            library: library.to_string(),
            install_dir: None,
            is_dlc: false,
            restart_steam: false,
            downloaded_bytes: 0,
            total_bytes: 0,
            reason: Some("queued by library import".into()),
            error_type: None,
            auto_attempts: 0,
            added_at: now,
            updated_at: now,
        });
    }
}

/// Drop `app_id` from the backlog; with `delete_files`, also delete its
/// partial install directory when that is provably safe. Returns the deleted
/// directory.
pub fn forget(app_id: u32, delete_files: bool) -> Result<Option<PathBuf>> {
    if is_running(app_id) {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            format!("app {app_id} is downloading; stop it first"),
        )
        .into());
    }
    if lock::is_held(app_id) {
        return Err(running_elsewhere(app_id));
    }
    let Some(entry) = backlog::get(app_id)? else {
        return Err(TypedError::new(
            ErrorKind::NotFound,
            format!("app {app_id} is not in the install backlog"),
        )
        .into());
    };
    let mut deleted: Option<PathBuf> = None;
    if delete_files {
        let Some(dir) = backlog::deletable_dir(&entry) else {
            bail!(
                "won't delete files for app {app_id}: its folder is unknown, shared with a base \
                 game, or belongs to a finished install. Remove it by hand if needed."
            );
        };
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("failed deleting {}", dir.display()))?;
        }
        deleted = Some(dir);
    }
    backlog::remove(app_id)?;
    Ok(deleted)
}

/// Whether some process holds `app_id`'s install lock right now.
pub fn lock_is_held(app_id: u32) -> bool {
    lock::is_held(app_id)
}

/// Uninstall a game, optionally deleting its Proton prefix too. Its shader
/// cache goes with it, as Steam does.
pub async fn uninstall(client: &SteamClient, app_id: u32, delete_prefix: bool) -> Result<()> {
    client
        .uninstall_game(app_id, delete_prefix)
        .await
        .with_context(|| format!("failed to uninstall app {app_id}"))?;
    if let Some(cache) = shader_cache(app_id)
        && let Err(e) = std::fs::remove_dir_all(&cache.path)
    {
        // The game is gone; a leftover cache is only disk space.
        tracing::warn!(path = %cache.path.display(), error = %e, "couldn't delete the shader cache");
    }
    Ok(())
}

/// A game's shader cache on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderCacheUsage {
    pub path: PathBuf,
    /// The size of the files in it.
    pub bytes: u64,
}

/// The game's shader cache, when `shader_cache` gives it one and it exists.
pub fn shader_cache(app_id: u32) -> Option<ShaderCacheUsage> {
    let settings = crate::settings::resolve_user(app_id, &Default::default()).ok()?;
    let path = settings.shader_cache_dir().filter(|dir| dir.is_dir())?;
    let bytes = crate::library::relocate::dir_size(&path);
    Some(ShaderCacheUsage { path, bytes })
}

/// Move an install to another library. Steam must be stopped.
pub async fn move_to<Fut: Future<Output = Result<bool>>>(
    client: &SteamClient,
    app_id: u32,
    library: PathBuf,
    drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    let rx = client
        .move_install(app_id, library)
        .await
        .with_context(|| format!("failed to start moving app {app_id}"))?;
    drive(rx)
        .await
        .with_context(|| format!("failed to move app {app_id}"))?;
    Ok(())
}

/// Point Steam at an install that was moved by hand. Steam must be stopped.
/// Returns the install directory.
pub async fn relink(client: &SteamClient, app_id: u32, library: PathBuf) -> Result<PathBuf> {
    client
        .relink_install(app_id, library)
        .await
        .with_context(|| format!("failed to relink app {app_id}"))
}

/// Register an existing copy of a game in a library. Steam must be stopped.
/// `platform` defaults to Linux, the host. Returns the install directory.
pub async fn import(
    client: &SteamClient,
    app_id: u32,
    library: PathBuf,
    platform: Option<DepotPlatform>,
) -> Result<PathBuf> {
    let platform = platform.unwrap_or(DepotPlatform::Linux);
    client
        .import_install(app_id, library, platform)
        .await
        .with_context(|| format!("failed to import app {app_id}"))
}

/// Whether a game is installed and playable, and its pin. Reads local files
/// only; needs no Steam session.
#[derive(Debug, Clone)]
pub struct Availability {
    pub available: bool,
    pub install_path: Option<String>,
    pub pinned: bool,
    pub pinned_manifests: HashMap<u32, u64>,
    /// The game's shader cache, if it has one on disk.
    pub shader_cache: Option<ShaderCacheUsage>,
}

/// See [`Availability`].
pub async fn availability(app_id: u32) -> Result<Availability> {
    // Deliberately no session restore: drivers call this per game on every
    // refresh, and Steam throttles repeated logons hard.
    let client = SteamClient::new()?;
    let (available, install_path) = client.is_game_available(app_id).await;
    let (pinned, pinned_manifests) = game_pin_state(app_id).await;
    Ok(Availability {
        available,
        install_path,
        pinned,
        pinned_manifests,
        shader_cache: shader_cache(app_id),
    })
}

/// Verify an install's files, repairing what is wrong.
pub async fn verify<Fut: Future<Output = Result<bool>>>(
    client: &SteamClient,
    app_id: u32,
    drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    let (_guard, state) = InstallGuard::begin(app_id)?;
    let rx = client
        .verify_game(app_id, state)
        .await
        .with_context(|| format!("failed to verify app {app_id}"))?;
    drive(rx).await?;
    Ok(())
}

/// Update a game. A pinned game is refused (`InvalidInput`) unless `force`.
pub async fn update<Fut: Future<Output = Result<bool>>>(
    client: &SteamClient,
    app_id: u32,
    force: bool,
    drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    let (pinned, _) = game_pin_state(app_id).await;
    if pinned && !force {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            format!("app {app_id} is pinned; unpin it first, or force the update"),
        )
        .into());
    }
    let (_guard, state) = InstallGuard::begin(app_id)?;
    let rx = client
        .update_game(app_id, state)
        .await
        .with_context(|| format!("failed to update app {app_id}"))?;
    drive(rx).await?;
    Ok(())
}

/// Installed games with a newer build, and pinned games (never reported as
/// updatable), each sorted by name.
#[derive(Debug, Clone, Default)]
pub struct UpdateCheck {
    /// Installed games checked.
    pub installed: usize,
    pub updates: Vec<LibraryGame>,
    pub pinned: Vec<LibraryGame>,
}

/// Check every installed game for updates. Fails `Offline` without Steam.
pub async fn check_updates(client: &mut SteamClient) -> Result<UpdateCheck> {
    if client.is_offline() {
        return Err(TypedError::new(
            ErrorKind::Offline,
            "offline — connect to Steam to check for updates",
        )
        .into());
    }
    let mut games = crate::ops::library::load_library(client).await;
    games.retain(|g| g.is_installed);
    if games.is_empty() {
        return Ok(UpdateCheck::default());
    }
    tracing::info!("Checking {} installed game(s) for updates ...", games.len());
    client.check_for_updates(&mut games).await?;

    let pins = crate::core::config::load_pins()?;
    let is_pinned = |app_id: u32| pins.contains_key(&app_id);
    let installed = games.len();
    let (mut pinned, rest): (Vec<LibraryGame>, Vec<LibraryGame>) =
        games.into_iter().partition(|g| is_pinned(g.app_id));
    let mut updates: Vec<LibraryGame> = rest.into_iter().filter(|g| g.update_available).collect();
    updates.sort_by_key(|g| g.name.to_lowercase());
    pinned.sort_by_key(|g| g.name.to_lowercase());
    Ok(UpdateCheck {
        installed,
        updates,
        pinned,
    })
}

/// Turn parallel depot / manifest lists (or `<depot>:<manifest>` entries)
/// into a depot → manifest map.
///
/// Bare manifests pair by position with `depots`; entries containing a `:`
/// carry their own depot. Rejects unequal parallel-list lengths, duplicate
/// depots, non-numeric ids and an empty result.
pub fn parse_manifest_overrides(depots: &[u32], manifests: &[String]) -> Result<HashMap<u32, u64>> {
    let mut map: HashMap<u32, u64> = HashMap::new();
    let mut bare: Vec<u64> = Vec::new();

    for m in manifests {
        if let Some((d, mm)) = m.split_once(':') {
            let depot: u32 = d
                .trim()
                .parse()
                .with_context(|| format!("invalid depot id in --manifest {m:?}"))?;
            let manifest: u64 = mm
                .trim()
                .parse()
                .with_context(|| format!("invalid manifest id in --manifest {m:?}"))?;
            if map.insert(depot, manifest).is_some() {
                bail!("depot {depot} specified more than once");
            }
        } else {
            let manifest: u64 = m.trim().parse().with_context(|| {
                format!("invalid manifest id {m:?} (use a number, or <depot>:<manifest>)")
            })?;
            bare.push(manifest);
        }
    }

    if !bare.is_empty() || !depots.is_empty() {
        if depots.len() != bare.len() {
            bail!(
                "--depot and --manifest must be given in equal numbers (got {} depot(s) and {} \
                 bare manifest(s)); pair each --depot with a --manifest, or use --manifest \
                 <depot>:<manifest>",
                depots.len(),
                bare.len()
            );
        }
        for (d, m) in depots.iter().zip(bare) {
            if map.insert(*d, m).is_some() {
                bail!("depot {d} specified more than once");
            }
        }
    }

    if map.is_empty() {
        bail!("provide at least one depot/manifest pair, e.g. --depot 1234 --manifest 5678");
    }
    Ok(map)
}

/// Each depot's current manifest per branch, optionally for one depot.
pub async fn manifests(
    client: &SteamClient,
    app_id: u32,
    depot: Option<u32>,
) -> Result<Vec<DepotManifestInfo>> {
    let mut manifests = client
        .list_depot_manifests(app_id)
        .await
        .with_context(|| format!("failed listing manifests for app {app_id}"))?;
    if let Some(d) = depot {
        manifests.retain(|m| m.depot_id == d);
    }
    Ok(manifests)
}

/// A downgrade: install specific depot manifests.
#[derive(Debug, Clone)]
pub struct Downgrade {
    pub app_id: u32,
    /// Depot → manifest to install.
    pub manifests: HashMap<u32, u64>,
    pub library: Option<String>,
    /// Branch whose build id is recorded in the appmanifest.
    pub branch: Option<String>,
    /// Verify the files afterwards.
    pub verify: bool,
    /// Pin the game at these manifests afterwards.
    pub pin: bool,
}

/// Install the requested manifests (then verify and pin, if asked). `drive`
/// is called for the download and again for the verify pass.
pub async fn downgrade<Fut: Future<Output = Result<bool>>>(
    client: &mut SteamClient,
    request: &Downgrade,
    mut drive: impl FnMut(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    let app_id = request.app_id;
    // Overridden depots are force-included regardless of platform; the
    // platform only picks the others.
    let (platforms, cached_vdf) = client
        .get_available_platforms(app_id)
        .await
        .context("failed to detect available platforms")?;
    let platform = platforms.first().copied().unwrap_or(DepotPlatform::Windows);

    let (_guard, state) = InstallGuard::begin(app_id)?;
    let options = InstallOptions {
        cached_vdf: Some(cached_vdf),
        library_override: request.library.clone(),
        manifest_overrides: Some(request.manifests.clone()),
        branch: request.branch.clone(),
        ..Default::default()
    };
    let rx = client
        .install_game(app_id, platform, options, state)
        .await
        .with_context(|| format!("failed to start downgrade for app {app_id}"))?;
    drive(rx).await?;

    // Checks against the just-written (downgraded) manifest ids.
    if request.verify {
        let state = Arc::new(RwLock::new(DownloadState::default()));
        let rx = client
            .verify_game(app_id, state)
            .await
            .with_context(|| format!("failed to verify app {app_id}"))?;
        drive(rx).await?;
    }

    if request.pin {
        set_game_pin(app_id, request.manifests.clone())
            .await
            .with_context(|| {
                format!("downgrade succeeded but failed to record the pin for app {app_id}")
            })?;
    }
    Ok(())
}

/// Pin a game at its installed manifests, so updates are refused. Returns
/// the pinned depot → manifest map.
pub async fn pin(app_id: u32) -> Result<HashMap<u32, u64>> {
    // Reading the local appmanifest needs no Steam session.
    let client = SteamClient::new()?;
    let manifests = client
        .installed_depot_manifests(app_id)
        .await
        .with_context(|| format!("failed reading installed manifests for app {app_id}"))?;
    if manifests.is_empty() {
        return Err(TypedError::new(
            ErrorKind::NotFound,
            format!("app {app_id} has no installed depots to pin (is it installed?)"),
        )
        .into());
    }
    set_game_pin(app_id, manifests.clone()).await?;
    Ok(manifests)
}

/// Release a game's pin. Returns whether it was pinned.
pub async fn unpin(app_id: u32) -> Result<bool> {
    let (was_pinned, _) = game_pin_state(app_id).await;
    clear_game_pin(app_id).await?;
    Ok(was_pinned)
}

#[cfg(test)]
mod tests;
