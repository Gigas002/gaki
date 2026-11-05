//! `tracing` subscriber setup. The TUI owns the terminal, so logs go only to
//! a file (see [`Settings::log_file`]), never to stdout or stderr.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing_subscriber::fmt;

use crate::settings::Settings;

#[cfg(test)]
mod tests;

/// Install the global subscriber. Call once, right after settings resolve.
/// With logging off, nothing is installed and no file is created.
pub fn init(settings: &Settings) -> Result<()> {
    if settings.log_level == tracing::level_filters::LevelFilter::OFF {
        return Ok(());
    }
    let file = open_log_file(&settings.log_file)?;
    fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_max_level(settings.log_level)
        .try_init()
        .map_err(|err| anyhow::anyhow!(err))
        .context("a tracing subscriber is already installed")
}

/// Open `path` for appending, creating it and its parent directory. The file
/// is private to the user: it can hold account names and Steam IDs.
fn open_log_file(path: &Path) -> Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))
}
