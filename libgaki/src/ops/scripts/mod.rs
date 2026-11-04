//! Per-game launch scripts.

use crate::core::error::{ErrorKind, TypedError};
use crate::launch::launch_script;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// One game with a launch script.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScriptEntry {
    pub app_id: u32,
    /// From the offline library cache, when known.
    pub name: Option<String>,
    /// The script that would run; `None` if it can't be resolved.
    pub path: Option<PathBuf>,
}

/// Every game with a launch script (dir-based or pinned in config).
pub async fn list() -> Result<Vec<ScriptEntry>> {
    let config = crate::core::config::load_launcher_config().await?;
    let library = crate::core::config::load_library_cache()
        .await
        .unwrap_or_default();
    Ok(launch_script::list_script_app_ids(Some(&config))
        .into_iter()
        .map(|app_id| ScriptEntry {
            app_id,
            name: library
                .iter()
                .find(|g| g.app_id == app_id)
                .map(|g| g.name.clone()),
            path: launch_script::resolve(app_id, Some(&config), None, false),
        })
        .collect())
}

/// Scaffold an executable launch script at `<script_dir>/<app_id>.sh`.
/// Refuses to overwrite an existing one unless `force`.
pub async fn create(app_id: u32, force: bool) -> Result<PathBuf> {
    let dir = launch_script::script_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed creating script dir {}", dir.display()))?;
    let path = launch_script::dir_script_path(app_id);
    if path.exists() && !force {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            format!("a launch script already exists at {}", path.display()),
        )
        .into());
    }
    let name = crate::ops::library::cached_game_name(app_id)
        .await
        .unwrap_or_default();
    std::fs::write(&path, launch_script::template(app_id, &name))
        .with_context(|| format!("failed writing {}", path.display()))?;

    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms)
        .with_context(|| format!("failed setting executable bit on {}", path.display()))?;
    Ok(path)
}

/// A game's resolved launch script.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScriptView {
    pub path: PathBuf,
    /// `None` when the resolved path doesn't exist on disk.
    pub contents: Option<String>,
}

/// The launch script `app_id` would run, with its contents; `None` if it has none.
pub async fn show(app_id: u32) -> Result<Option<ScriptView>> {
    let config = crate::core::config::load_launcher_config().await?;
    Ok(
        launch_script::resolve(app_id, Some(&config), None, false).map(|path| ScriptView {
            contents: std::fs::read_to_string(&path).ok(),
            path,
        }),
    )
}

/// Delete `app_id`'s dir-based launch script. Returns its path and whether it
/// existed.
pub fn remove(app_id: u32) -> Result<(PathBuf, bool)> {
    let removed = launch_script::remove_dir_script(app_id)
        .with_context(|| format!("failed removing launch script for app {app_id}"))?;
    Ok((launch_script::dir_script_path(app_id), removed))
}
