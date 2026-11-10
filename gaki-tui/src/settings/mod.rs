//! Resolves [`CliOptions`] and gaki's config into one [`Settings`]. Nothing
//! below this module sees the raw CLI or reads config to decide how to start.

use std::path::PathBuf;

use anyhow::Result;
use libgaki::core::config::ProxyConfig;
use tracing::level_filters::LevelFilter;

use crate::cli::{CliOptions, LogLevel};

#[cfg(test)]
mod tests;

/// The log file's name inside gaki's state directory.
pub const LOG_FILE_NAME: &str = "gaki-tui.log";

/// What the TUI opens on start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// The library browser.
    Library,
    /// A full-screen chat with one friend.
    #[cfg(feature = "social")]
    Chat { steamid: u64, name: String },
}

/// The TUI's fully resolved settings.
#[derive(Debug, Clone)]
pub struct Settings {
    pub start: Start,
    pub log_level: LevelFilter,
    pub log_file: PathBuf,
    pub proxy: ProxyConfig,
    /// The theme file asked for with `--theme`; without it, the default
    /// path, which may be missing.
    pub theme: Option<PathBuf>,
}

impl Settings {
    /// Resolve against the real environment: gaki's state dir and config.
    pub fn resolve(cli: CliOptions) -> Result<Self> {
        let log_file = libgaki::core::config::state_dir()?.join(LOG_FILE_NAME);
        let proxy = libgaki::core::config::load_proxy_config_blocking();
        Ok(Self::merge(cli, log_file, proxy))
    }

    /// The CLI layer over the defaults. Pure, so tests needn't touch disk.
    pub fn merge(cli: CliOptions, log_file: PathBuf, proxy: ProxyConfig) -> Self {
        #[cfg(feature = "social")]
        let start = match cli.chat {
            Some(steamid) => Start::Chat {
                steamid,
                name: cli.name.unwrap_or_default(),
            },
            None => Start::Library,
        };
        #[cfg(not(feature = "social"))]
        let start = Start::Library;
        let log_level = cli.log_level.map_or(LevelFilter::INFO, level_filter);
        Self {
            start,
            log_level,
            log_file,
            proxy,
            theme: cli.theme,
        }
    }
}

fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Off => LevelFilter::OFF,
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Trace => LevelFilter::TRACE,
    }
}
