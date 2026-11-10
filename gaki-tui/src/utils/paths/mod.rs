use crate::utils::error::TuiError;

use std::fs::File;
use std::io::Write;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const CONFIG_FILE: &str = "config.toml";

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

fn mkdir(dir: PathBuf) -> Result<PathBuf, TuiError> {
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn libgaki_dir(dir: anyhow::Result<PathBuf>) -> Result<PathBuf, TuiError> {
    dir.map_err(|err| TuiError::Problem(format!("{err:#}")))
}

/// `gaki/tui/` under the cache dir: the cached library and cover art.
pub fn cache_directory() -> Result<PathBuf, TuiError> {
    mkdir(libgaki_dir(libgaki::core::config::cache_dir())?.join(SUBDIR))
}

/// `gaki/tui/` under the config dir.
pub fn config_directory() -> Result<PathBuf, TuiError> {
    mkdir(libgaki_dir(libgaki::core::config::config_dir())?.join(SUBDIR))
}

/// `keybinds.toml` in [`config_directory`]; it may not exist.
pub fn keybinds_file() -> Result<PathBuf, TuiError> {
    Ok(config_directory()?.join("keybinds.toml"))
}

/// `theme.toml` in gaki's config dir (not `tui/`); it may not exist.
pub fn theme_file() -> Result<PathBuf, TuiError> {
    Ok(libgaki_dir(libgaki::core::config::config_dir())?.join("theme.toml"))
}

fn icon_directory() -> Result<PathBuf, TuiError> {
    mkdir(cache_directory()?.join("icons"))
}

pub fn icon_exists(id: i32, suffix: &str) -> Result<PathBuf, TuiError> {
    let dir = icon_directory()?;
    let icon = &format!("{}{}.ico", id, suffix);
    let icon = Path::new(icon);
    let icon = dir.join(icon);
    if icon.exists() {
        Ok(icon)
    } else {
        Err(TuiError::Problem(format!("Icon doesn't exist: {:?}", icon)))
    }
}

pub fn icon_save(id: i32, suffix: &str, icon: &[u8]) -> Result<(), TuiError> {
    let dir = icon_directory()?;
    let icon_path = &format!("{}{}.ico", id, suffix);
    let icon_path = Path::new(icon_path);
    let icon_path = dir.join(icon_path);
    let mut file = File::create(icon_path)?;
    file.write_all(icon)?;
    Ok(())
}

pub fn config_location() -> Result<PathBuf, TuiError> {
    let dir = config_directory()?;
    let config_path = Path::new(CONFIG_FILE);
    let config_path = dir.join(config_path);
    touch(&config_path)?;
    Ok(config_path)
}

pub fn cache_location() -> Result<PathBuf, TuiError> {
    let dir = cache_directory()?;
    let cache_path = Path::new("games.toml");
    let cache_path = dir.join(cache_path);
    touch(&cache_path)?;
    Ok(cache_path)
}

pub fn invalidate_cache() -> Result<(), TuiError> {
    fs::remove_file(cache_location()?)?;
    Ok(())
}
