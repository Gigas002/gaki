//! The user's XDG folders (`user-dirs.dirs`: Downloads, Pictures, ...) and
//! `~` in typed directories.

use std::path::{Path, PathBuf};

/// The folder `key` (`DOWNLOAD`, `PICTURES`, ...) from `user-dirs.dirs`,
/// else `~/<fallback>`, as text for a prompt.
pub fn user_dir(key: &str, fallback: &str) -> String {
    let home = libgaki::core::config::home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    std::fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| parse_user_dir(&text, key, &home))
        .unwrap_or_else(|| home.join(fallback))
        .display()
        .to_string()
}

/// `XDG_<key>_DIR` from the text of a `user-dirs.dirs` file, with `$HOME`
/// expanded. Only `$HOME/...` and absolute paths are valid there.
pub fn parse_user_dir(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let prefix = format!("XDG_{key}_DIR=");
    let value = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix(prefix.as_str())
            .map(|v| v.trim().trim_matches('"').to_string())
    })?;
    if let Some(rest) = value.strip_prefix("$HOME") {
        Some(home.join(rest.trim_start_matches('/')))
    } else if value.starts_with('/') {
        Some(PathBuf::from(value))
    } else {
        None
    }
}

/// A typed directory with `~` expanded.
pub fn expand_dir(input: &str, home: &Path) -> PathBuf {
    let input = input.trim();
    if input == "~" {
        home.to_path_buf()
    } else if let Some(rest) = input.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(input)
    }
}

#[cfg(test)]
mod tests;
