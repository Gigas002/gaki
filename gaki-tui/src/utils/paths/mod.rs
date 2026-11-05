use crate::utils::error::STError;

use std::fs::File;
use std::io::Write;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const CONFIG_FILE: &str = "config.json";

#[cfg(test)]
mod tests;

fn touch(path: &Path) -> io::Result<()> {
    match fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

/// The TUI's own files live in a `tui/` subdirectory of gaki's dirs.
const SUBDIR: &str = "tui";
/// Where the TUI kept its config before it moved under gaki's config dir.
const LEGACY_CONFIG_DIR: &str = ".config/aurelia-tui";

fn mkdir(dir: PathBuf) -> Result<PathBuf, STError> {
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn libgaki_dir(dir: anyhow::Result<PathBuf>) -> Result<PathBuf, STError> {
    dir.map_err(|err| STError::Problem(format!("{err:#}")))
}

/// `gaki/tui/` under the cache dir: the cached library and cover art.
pub fn cache_directory() -> Result<PathBuf, STError> {
    mkdir(libgaki_dir(libgaki::core::config::cache_dir())?.join(SUBDIR))
}

/// `gaki/tui/` under the config dir. On first use, a `config.json` from the
/// old `~/.config/aurelia-tui` is copied in, so favourites and hidden games
/// carry over. A config dir set for tests (`set_config_dir`) skips that.
pub fn config_directory() -> Result<PathBuf, STError> {
    let dir = mkdir(libgaki_dir(libgaki::core::config::config_dir())?.join(SUBDIR))?;
    if libgaki::core::config::config_dir_override().is_none()
        && let Ok(home) = libgaki::core::config::home_dir()
    {
        adopt_legacy_config(&home.join(LEGACY_CONFIG_DIR), &dir)?;
    }
    Ok(dir)
}

/// Copy `legacy/config.json` into `dir` if `dir` has none yet. Returns
/// whether it copied. The old file stays, so going back to an older build
/// still works.
fn adopt_legacy_config(legacy: &Path, dir: &Path) -> io::Result<bool> {
    let (old, new) = (legacy.join(CONFIG_FILE), dir.join(CONFIG_FILE));
    if new.exists() || !old.is_file() {
        return Ok(false);
    }
    fs::copy(&old, &new)?;
    tracing::info!(from = %old.display(), to = %new.display(), "adopted the old TUI config");
    Ok(true)
}

fn icon_directory() -> Result<PathBuf, STError> {
    mkdir(cache_directory()?.join("icons"))
}

pub fn icon_exists(id: i32, suffix: &str) -> Result<PathBuf, STError> {
    let dir = icon_directory()?;
    let icon = &format!("{}{}.ico", id, suffix);
    let icon = Path::new(icon);
    let icon = dir.join(icon);
    if icon.exists() {
        Ok(icon)
    } else {
        Err(STError::Problem(format!("Icon doesn't exist: {:?}", icon)))
    }
}

pub fn icon_save(id: i32, suffix: &str, icon: &[u8]) -> Result<(), STError> {
    let dir = icon_directory()?;
    let icon_path = &format!("{}{}.ico", id, suffix);
    let icon_path = Path::new(icon_path);
    let icon_path = dir.join(icon_path);
    let mut file = File::create(icon_path)?;
    file.write_all(icon)?;
    Ok(())
}

pub fn config_location() -> Result<PathBuf, STError> {
    let dir = config_directory()?;
    let config_path = Path::new(CONFIG_FILE);
    let config_path = dir.join(config_path);
    touch(&config_path)?;
    Ok(config_path)
}

pub fn cache_location() -> Result<PathBuf, STError> {
    let dir = cache_directory()?;
    let cache_path = Path::new("games.json");
    let cache_path = dir.join(cache_path);
    touch(&cache_path)?;
    Ok(cache_path)
}

pub fn invalidate_cache() -> Result<(), STError> {
    fs::remove_file(cache_location()?)?;
    Ok(())
}
