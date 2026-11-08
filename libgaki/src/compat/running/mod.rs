//! Tracks games launched by gaki so a separate `gaki-cli stop <app_id>`
//! invocation can find and terminate them.
//!
//! `gaki-cli play` blocks in the foreground process while the game runs, so the
//! PID of the launched process can't be queried from another invocation. To make
//! `stop` work across processes, each launch records a small TOML file under
//! `~/.local/state/gaki/running/<app_id>.toml` holding the child PID (and, for a
//! per-game Proton/Wine launch, the WINEPREFIX the game runs in). The launching
//! process removes the file when the game exits.
//!
//! # Recovery contract
//!
//! A record can outlive its launch: `gaki-cli play` may be killed or crash, the
//! machine may reboot, and PIDs are recycled. A record is therefore never trusted
//! on its PID alone:
//!
//! - Each record pairs the PID with that process's start time. The recorded PID
//!   is only treated as the game (listed as running, or signalled by `stop`)
//!   while a process with *both* the same PID and the same start time exists.
//! - A record whose PID no longer matches is reconciled against the processes
//!   carrying the app id in their environment (see [`processes_for_app`]). If
//!   one survives, the record is re-pointed at it; otherwise the record is stale
//!   and is removed without signalling anything.
//! - Records written before start times were tracked carry none and are
//!   reconciled the same way, never acted on by PID.
//! - Records are replaced atomically (write to a temp file, then rename), so a
//!   crash mid-write leaves the previous record rather than a truncated one.

use crate::core::config::running_games_dir;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningGame {
    pub app_id: u32,
    pub name: String,
    /// PID of the process gaki spawned for this game: the game executable for a
    /// native launch, the runner process for a Proton/Wine launch.
    pub pid: u32,
    /// Start time of `pid` as the OS reports it (seconds since the Unix epoch).
    /// Together with `pid` this identifies the process, since PIDs are recycled.
    /// `None` when it could not be read, or on records written before it existed;
    /// such a record never authorises signalling `pid`.
    #[serde(default)]
    pub pid_start_time: Option<u64>,
    /// The per-game (compatdata) WINEPREFIX the game runs in, when launched
    /// through Proton/Wine. Lets `stop` sweep leftover wine processes inside
    /// the prefix.
    #[serde(default)]
    pub wineprefix: Option<PathBuf>,
}

impl RunningGame {
    /// A record for a just-spawned process, capturing its start time.
    pub fn new(app_id: u32, name: String, pid: u32, wineprefix: Option<PathBuf>) -> Self {
        Self {
            app_id,
            name,
            pid,
            pid_start_time: process_start_time(pid),
            wineprefix,
        }
    }

    /// Re-point the record at `pid` (a surviving game process), refreshing the
    /// start time to match.
    pub fn set_pid(&mut self, pid: u32) {
        self.pid = pid;
        self.pid_start_time = process_start_time(pid);
    }

    /// Whether the recorded PID still names the process this record was written
    /// for: same PID *and* same start time. False for a record without a start
    /// time, so a recycled PID is never mistaken for the game.
    pub fn owns_pid(&self) -> bool {
        self.pid_start_time
            .is_some_and(|t| process_start_time(self.pid) == Some(t))
    }
}

/// Start time of `pid` (seconds since the Unix epoch), or `None` if no such
/// process exists.
pub fn process_start_time(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(pid);
    let mut sys = System::new().ok()?;
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).map(|p| p.start_time())
}

fn running_dir() -> Result<PathBuf> {
    running_games_dir()
}

fn record_path(app_id: u32) -> Result<PathBuf> {
    Ok(running_dir()?.join(format!("{app_id}.toml")))
}

/// Persist that `game` is now running so `stop` can find it later. Written
/// atomically, so a reader (or a crash) never sees a half-written record.
pub fn record_launch(game: &RunningGame) -> Result<()> {
    crate::core::store::write(&record_path(game.app_id)?, game)
}

/// Load the running-game record for `app_id`, if one exists.
pub fn load(app_id: u32) -> Option<RunningGame> {
    crate::core::store::read(&record_path(app_id).ok()?)
        .ok()
        .flatten()
}

/// Remove the running-game record for `app_id` (no-op if absent).
pub fn clear(app_id: u32) {
    if let Ok(path) = record_path(app_id) {
        let _ = std::fs::remove_file(path);
    }
}

/// Every game gaki currently has a record for, live or stale. Use
/// [`list_active`] for the ones still running.
pub fn list() -> Vec<RunningGame> {
    let Ok(dir) = running_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut games: Vec<RunningGame> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("toml"))
        .filter_map(|path| crate::core::store::read(&path).ok().flatten())
        .collect();
    games.sort_by(|a, b| a.name.cmp(&b.name));
    games
}

/// Every live process belonging to `app_id`, identified by
/// `STEAM_COMPAT_APP_ID=<id>` (Proton) or `SteamAppId=<id>` (native launches)
/// in its environment. The recorded PID alone is not authoritative: Proton
/// re-parents the game away from the spawned runner, and script launchers
/// (`hl.sh`-style) exit while the game keeps running.
pub fn processes_for_app(app_id: u32) -> Vec<u32> {
    let compat = format!("STEAM_COMPAT_APP_ID={app_id}");
    let native = format!("SteamAppId={app_id}");
    let mut pids = Vec::new();
    let Ok(proc_dir) = std::fs::read_dir("/proc") else {
        return pids;
    };
    for entry in proc_dir.flatten() {
        let path = entry.path();
        let Some(pid) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(environ) = std::fs::read(path.join("environ")) else {
            continue;
        };
        // NUL-separated KEY=VALUE entries; exact match so 70 never matches 700.
        if environ
            .split(|&b| b == 0)
            .any(|e| e == compat.as_bytes() || e == native.as_bytes())
        {
            pids.push(pid);
        }
    }
    pids
}

/// The games gaki is running whose process is still alive. Records whose
/// process has already exited are pruned as a side effect, so stale launches
/// (an interrupted `gaki-cli play`, a reboot) don't linger in the listing.
///
/// A record that no longer owns its PID does not mean the game is gone — the
/// record holds the directly spawned process, which wrappers and script launchers
/// outlive — so it is only pruned once no process carries the app id either.
/// When the game survives its wrapper, the record is re-pointed at a live
/// process so `stop` still has a target.
pub fn list_active() -> Vec<RunningGame> {
    list().into_iter().filter_map(reconcile).collect()
}

/// Apply the recovery contract to one record: keep it while it owns its PID,
/// re-point it at a surviving app process, or remove it as stale (`None`).
pub fn reconcile(mut game: RunningGame) -> Option<RunningGame> {
    if game.owns_pid() {
        return Some(game);
    }
    match processes_for_app(game.app_id).first() {
        Some(&pid) => {
            game.set_pid(pid);
            let _ = record_launch(&game);
            Some(game)
        }
        None => {
            clear(game.app_id);
            None
        }
    }
}

#[cfg(test)]
mod tests;
