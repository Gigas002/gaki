//! Shader pre-caching, as the official client does it.
//!
//! Steam keeps pre-built shader data per game in *buckets*, each a manifest
//! of the depot whose id is the game's app id (`Shader.GetBucketManifest`).
//! gaki fetches two of them into the game's shader cache (`shader_cache`,
//! the directory the launch points DXVK, VKD3D-Proton and the drivers at):
//!
//! * the Vulkan pipelines every player's runs recorded (`SteamSwarm` /
//!   `G7:VulkanPipelinesV6`): Fossilize databases under `fozpipelinesv6/`.
//!   `fossilize_replay` then compiles them with the launch's cache variables,
//!   so the driver's cache already holds them when the game starts;
//! * transcoded video for Proton's media converter (`SteamSwarm` /
//!   `G7:CompatVideoTCMediaV1`): `transcoded_video.foz`, which a Proton
//!   launch hands over as `STEAM_COMPAT_TRANSCODED_MEDIA_PATH`. Skipped for a
//!   native Linux install.
//!
//! What was fetched is kept in `gaki-precache.toml` in that directory, so an
//! unchanged bucket isn't fetched or replayed again. gaki never uploads
//! pipelines or media of its own.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::core::error::{ErrorKind, TypedError};
use crate::library::depot_browser::EntryKind;
use crate::steam_client::SteamClient;

/// The file in a game's shader cache that records the fetched buckets.
pub const STATE_FILE: &str = "gaki-precache.toml";

/// The `fossilize_replay` that ships with the official client, under its
/// install directory.
const STEAM_REPLAYER: &str = "ubuntu12_64/fossilize_replay";

/// A kind of bucket gaki fetches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BucketKind {
    /// Recorded Vulkan pipelines, compiled afterwards.
    Pipelines,
    /// Transcoded video for Proton.
    Video,
}

impl BucketKind {
    pub const ALL: [BucketKind; 2] = [BucketKind::Pipelines, BucketKind::Video];

    /// The GPU and driver descriptions Steam files the bucket under.
    pub fn descs(self) -> (&'static str, &'static str) {
        match self {
            BucketKind::Pipelines => ("SteamSwarm", "G7:VulkanPipelinesV6"),
            BucketKind::Video => ("SteamSwarm", "G7:CompatVideoTCMediaV1"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BucketKind::Pipelines => "pipelines",
            BucketKind::Video => "video",
        }
    }
}

/// What happened to one bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BucketStatus {
    /// Steam has none for this game.
    NotOffered,
    /// Not wanted for this install (video for a native Linux build).
    Skipped,
    /// The files on disk are the current manifest's.
    Current,
    /// Fetched now.
    Downloaded {
        files: usize,
        bytes: u64,
    },
    Failed {
        error: String,
    },
}

/// One bucket's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BucketResult {
    pub kind: BucketKind,
    pub manifest_id: Option<u64>,
    #[serde(flatten)]
    pub status: BucketStatus,
}

/// What the compile step did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Replay {
    /// `fossilize_replay` ran over every pipeline database.
    Compiled {
        graphics: u64,
        compute: u64,
    },
    /// These pipelines were compiled before; nothing changed since.
    UpToDate,
    /// No pipeline database to compile.
    NoPipelines,
    /// Neither Steam's `fossilize_replay` nor one on `PATH`.
    NoReplayer,
    /// Not asked for (download only).
    Skipped,
    Failed {
        error: String,
    },
}

/// The outcome of [`precache`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub app_id: u32,
    /// The game's shader cache.
    pub dir: PathBuf,
    pub buckets: Vec<BucketResult>,
    pub replay: Replay,
}

impl Report {
    /// One line: each bucket, then the compile step.
    pub fn summary(&self) -> String {
        let buckets: Vec<String> = self
            .buckets
            .iter()
            .map(|b| {
                let what = match &b.status {
                    BucketStatus::NotOffered => "none on Steam".to_owned(),
                    BucketStatus::Skipped => "not needed".to_owned(),
                    BucketStatus::Current => "up to date".to_owned(),
                    BucketStatus::Downloaded { bytes, .. } => {
                        format!("downloaded ({})", crate::core::utils::human_bytes(*bytes))
                    }
                    BucketStatus::Failed { error } => format!("failed: {error}"),
                };
                format!("{}: {what}", b.kind.name())
            })
            .collect();
        let replay = match &self.replay {
            Replay::Compiled { graphics, compute } => {
                format!("compiled {graphics} graphics and {compute} compute pipelines")
            }
            Replay::UpToDate => "already compiled".to_owned(),
            Replay::NoPipelines => "nothing to compile".to_owned(),
            Replay::NoReplayer => "not compiled: no fossilize_replay".to_owned(),
            Replay::Skipped => "not compiled".to_owned(),
            Replay::Failed { error } => format!("compile failed: {error}"),
        };
        format!("Shaders: {}; {replay}.", buckets.join(", "))
    }

    /// A few words for a status line.
    pub fn short(&self) -> &'static str {
        let failed = self
            .buckets
            .iter()
            .any(|b| matches!(b.status, BucketStatus::Failed { .. }));
        match &self.replay {
            Replay::Failed { .. } => "shader compile failed",
            _ if failed => "shader download failed",
            Replay::Compiled { .. } => "shaders compiled",
            Replay::UpToDate => "shaders up to date",
            Replay::NoPipelines => "no shaders on Steam",
            Replay::NoReplayer => "shaders fetched; no fossilize_replay",
            Replay::Skipped => "shaders fetched",
        }
    }
}

/// How [`precache`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Compile the pipelines after fetching them.
    pub replay: bool,
    /// Compile them even when these exact pipelines were compiled before
    /// (after a driver update, say).
    pub force_replay: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            replay: true,
            force_replay: false,
        }
    }
}

/// `gaki-precache.toml`: per bucket kind, the manifest on disk, its files,
/// and the manifest last compiled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct State {
    #[serde(default, flatten)]
    pub buckets: BTreeMap<String, BucketState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BucketState {
    pub manifest_id: u64,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replayed: Option<u64>,
}

impl State {
    fn load(dir: &Path) -> State {
        std::fs::read_to_string(dir.join(STATE_FILE))
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &Path) -> Result<()> {
        let text = toml::to_string(self).context("failed serializing the pre-cache state")?;
        let path = dir.join(STATE_FILE);
        std::fs::write(&path, text).with_context(|| format!("failed writing {}", path.display()))
    }

    /// Whether `kind`'s files for `manifest_id` are all on disk.
    fn is_current(&self, kind: BucketKind, manifest_id: u64, dir: &Path) -> bool {
        self.buckets.get(kind.name()).is_some_and(|b| {
            b.manifest_id == manifest_id
                && b.files
                    .iter()
                    .all(|f| safe_join(dir, f).is_ok_and(|p| p.is_file()))
        })
    }
}

/// `depot_path` under `dir`. A path with no name, or with an empty, `.` or
/// `..` part, is refused, so a manifest can't write outside `dir`. Both
/// separators count: Steam lists these paths with `\` too.
pub(crate) fn safe_join(dir: &Path, depot_path: &str) -> Result<PathBuf> {
    let mut path = dir.to_path_buf();
    let mut parts = 0;
    for part in depot_path.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." {
            bail!("unsafe path in a shader bucket: {depot_path:?}");
        }
        path.push(part);
        parts += 1;
    }
    if parts == 0 {
        bail!("empty path in a shader bucket");
    }
    Ok(path)
}

/// The `fossilize_replay` to compile with: the official client's, else the
/// first on `PATH`.
pub fn find_replayer() -> Option<PathBuf> {
    let steam = crate::core::config::detect_steam_path()
        .map(|root| root.join(STEAM_REPLAYER))
        .filter(|p| p.is_file());
    steam.or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("fossilize_replay"))
            .find(|p| p.is_file())
    })
}

/// The variables `fossilize_replay` runs with: the launch's shader cache
/// variables for `dir`, so the driver caches what it compiles where the game
/// will look. Like the launch, one set in gaki's own environment
/// (`inherited`) is left alone and the profile's and game's `env` win.
pub(crate) fn replay_env(
    dir: &Path,
    inherited: impl Fn(&str) -> bool,
    user_env: &BTreeMap<String, String>,
) -> Vec<(String, OsString)> {
    crate::config::ShaderCache::env(dir)
        .into_iter()
        .filter_map(|(key, value)| match user_env.get(key) {
            Some(own) => Some((key.to_owned(), OsString::from(own))),
            None if inherited(key) => None,
            None => Some((key.to_owned(), value.into_os_string())),
        })
        .collect()
}

/// The last `Compile graphics N / M` and `Compile compute N / M` counts in
/// `fossilize_replay --progress` output.
pub(crate) fn compiled_counts(output: &str) -> (u64, u64) {
    let count = |label: &str| {
        output
            .lines()
            .rev()
            .filter_map(|line| line.split_once(label))
            .find_map(|(_, rest)| rest.split_whitespace().next()?.parse().ok())
            .unwrap_or(0)
    };
    (count("Compile graphics "), count("Compile compute "))
}

/// Run `fossilize_replay` over each database in turn.
async fn run_replayer(
    replayer: &Path,
    databases: &[PathBuf],
    env: &[(String, OsString)],
) -> Replay {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get() / 2)
        .max(1)
        .to_string();
    let (mut graphics, mut compute) = (0, 0);
    for db in databases {
        tracing::info!(db = %db.display(), "compiling shader pipelines");
        let output = tokio::process::Command::new(replayer)
            .args(["--master-process", "--quiet-slave", "--progress"])
            .args(["--num-threads", &threads])
            .arg(db)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(e) => {
                return Replay::Failed {
                    error: format!("couldn't run {}: {e}", replayer.display()),
                };
            }
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            let last = text.lines().rev().find(|l| !l.trim().is_empty());
            return Replay::Failed {
                error: format!(
                    "fossilize_replay {} on {}{}",
                    output.status,
                    db.display(),
                    last.map(|l| format!(": {}", l.trim())).unwrap_or_default()
                ),
            };
        }
        let (g, c) = compiled_counts(&text);
        graphics += g;
        compute += c;
    }
    Replay::Compiled { graphics, compute }
}

/// Fetch `kind`'s current bucket into `dir`, unless it is already there.
async fn fetch(
    client: &SteamClient,
    app_id: u32,
    dir: &Path,
    kind: BucketKind,
    state: &mut State,
) -> Result<BucketResult> {
    let (gpu, driver) = kind.descs();
    let Some(bucket) = client.shader_bucket(app_id, gpu, driver).await? else {
        return Ok(BucketResult {
            kind,
            manifest_id: None,
            status: BucketStatus::NotOffered,
        });
    };
    let manifest_id = bucket.manifest_id;
    let result = |status| BucketResult {
        kind,
        manifest_id: Some(manifest_id),
        status,
    };
    if state.is_current(kind, manifest_id, dir) {
        return Ok(result(BucketStatus::Current));
    }

    // The bucket's depot is the app itself.
    let entries = client
        .depot_manifest_files(app_id, app_id, manifest_id)
        .await
        .with_context(|| format!("failed reading the {} bucket", kind.name()))?;
    let mut files = Vec::new();
    let mut bytes = 0;
    for entry in entries.iter().filter(|e| e.kind == EntryKind::File) {
        let dest = safe_join(dir, &entry.path)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed creating {}", parent.display()))?;
        }
        bytes += client
            .download_depot_file(app_id, app_id, manifest_id, &entry.path, &dest, true)
            .await?;
        files.push(entry.path.clone());
    }

    // Files the previous manifest had and this one doesn't.
    let previous = state.buckets.remove(kind.name()).unwrap_or_default();
    for old in previous.files.iter().filter(|f| !files.contains(f)) {
        if let Ok(path) = safe_join(dir, old) {
            let _ = std::fs::remove_file(path);
        }
    }
    let count = files.len();
    state.buckets.insert(
        kind.name().to_owned(),
        BucketState {
            manifest_id,
            files,
            // Kept, so a changed manifest reads as not yet compiled.
            replayed: previous.replayed,
        },
    );
    Ok(result(BucketStatus::Downloaded {
        files: count,
        bytes,
    }))
}

/// Whether `app_id` is installed as a native Linux build (no Proton, so no
/// use for the video bucket).
async fn is_native_linux(app_id: u32) -> bool {
    let installed = crate::library::scan_installed_app_info()
        .await
        .unwrap_or_default();
    installed
        .get(&app_id)
        .and_then(|info| {
            crate::library::platform::detect_installed_platform(
                &info.install_path.to_string_lossy(),
            )
        })
        .is_some_and(|p| p == "linux")
}

/// Fetch Steam's pre-built shaders for `app_id` into its shader cache and
/// compile the pipelines. Needs a connection. Fails `InvalidInput` when
/// `shader_cache` is `off` for the game.
pub async fn precache(client: &SteamClient, app_id: u32, options: Options) -> Result<Report> {
    let settings = crate::settings::resolve_user(app_id, &Default::default())?;
    let dir = settings.shader_cache_dir().ok_or_else(|| {
        TypedError::new(
            ErrorKind::InvalidInput,
            format!("app {app_id} has `shader_cache = \"off\"`, so there is nowhere to pre-cache"),
        )
    })?;
    std::fs::create_dir_all(&dir).with_context(|| format!("failed creating {}", dir.display()))?;

    let mut state = State::load(&dir);
    let native = is_native_linux(app_id).await;
    let mut buckets = Vec::new();
    for kind in BucketKind::ALL {
        if kind == BucketKind::Video && native {
            buckets.push(BucketResult {
                kind,
                manifest_id: None,
                status: BucketStatus::Skipped,
            });
            continue;
        }
        let result = match fetch(client, app_id, &dir, kind, &mut state).await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(app_id, bucket = kind.name(), "shader bucket: {e:#}");
                BucketResult {
                    kind,
                    manifest_id: None,
                    status: BucketStatus::Failed {
                        error: format!("{e:#}"),
                    },
                }
            }
        };
        buckets.push(result);
    }
    state.save(&dir)?;

    let pipelines = state
        .buckets
        .get(BucketKind::Pipelines.name())
        .cloned()
        .unwrap_or_default();
    let databases: Vec<PathBuf> = pipelines
        .files
        .iter()
        .filter(|f| f.ends_with(".foz"))
        .filter_map(|f| safe_join(&dir, f).ok())
        .collect();
    let replay = if !options.replay {
        Replay::Skipped
    } else if databases.is_empty() {
        Replay::NoPipelines
    } else if !options.force_replay && pipelines.replayed == Some(pipelines.manifest_id) {
        Replay::UpToDate
    } else if let Some(replayer) = find_replayer() {
        if let Err(e) = crate::config::ShaderCache::create_dirs(&dir) {
            tracing::warn!(dir = %dir.display(), "can't create the shader cache: {e}");
        }
        let env = replay_env(&dir, |k| std::env::var_os(k).is_some(), &settings.env);
        let replay = run_replayer(&replayer, &databases, &env).await;
        if matches!(replay, Replay::Compiled { .. })
            && let Some(bucket) = state.buckets.get_mut(BucketKind::Pipelines.name())
        {
            bucket.replayed = Some(bucket.manifest_id);
            state.save(&dir)?;
        }
        replay
    } else {
        Replay::NoReplayer
    };

    Ok(Report {
        app_id,
        dir,
        buckets,
        replay,
    })
}

/// Whether an install or update of `app_id` should be followed by
/// [`precache`]: `shader_precache` is on (the default) and the game has a
/// shader cache.
pub fn wanted_after_install(app_id: u32) -> bool {
    let enabled = crate::ops::config::global()
        .map(|c| c.shader_precache)
        .unwrap_or(true);
    enabled
        && crate::settings::resolve_user(app_id, &Default::default())
            .is_ok_and(|s| s.shader_cache_dir().is_some())
}

/// [`precache`] after an install or update, when
/// [`wanted_after_install`]. `None` when it doesn't run; a failure is the
/// caller's to report, never the install's.
pub async fn after_install(client: &SteamClient, app_id: u32) -> Option<Result<Report>> {
    if !wanted_after_install(app_id) {
        return None;
    }
    Some(precache(client, app_id, Options::default()).await)
}

#[cfg(test)]
mod tests;
