//! `config.toml`: settings that apply to every game.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    Platform,
    issue::{Issue, Issues, Subject, ValidationContext},
};
use crate::{core::config::ChatPresence, library::backlog::InstallResume};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The Steam library games install into. `None` is the detected Steam
    /// library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_library: Option<PathBuf>,
    /// Where profiles live. `None` is `<config dir>/profiles`; pointing it at
    /// `~/.config/lootrs/profiles` shares lootrs' profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles_dir: Option<PathBuf>,
    /// The profile used when a game names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    /// The build to install and run when a game has both. `None` installs the
    /// Windows build and runs whichever build is installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_platform: Option<Platform>,
    #[serde(default = "default_true")]
    pub cloud_sync: bool,
    /// Steam presence while gaki is signed in.
    #[serde(default)]
    pub presence: ChatPresence,
    /// Proxy URL for all HTTP(S) traffic, e.g. `socks5://127.0.0.1:1080`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// Comma-separated hosts that bypass the proxy (`NO_PROXY` semantics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<String>,
    /// Whether the daemon resumes interrupted installs on start.
    #[serde(default)]
    pub install_resume: InstallResume,
    /// Opt-in gate for experimental features.
    #[serde(default)]
    pub experimental: bool,
    /// Directory of the old per-app-id `<app id>.sh` scripts, the fallback when
    /// a game sets no `script`. `None` is `<config dir>/scripts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_dir: Option<PathBuf>,
    /// Lifetime of a cached `info` record in seconds. `None` is 6 hours, `0`
    /// disables the cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info_cache_ttl: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            steam_library: None,
            profiles_dir: None,
            default_profile: None,
            default_platform: None,
            cloud_sync: true,
            presence: ChatPresence::default(),
            proxy: None,
            no_proxy: None,
            install_resume: InstallResume::default(),
            experimental: false,
            script_dir: None,
            info_cache_ttl: None,
        }
    }
}

impl Config {
    /// Checks this file on its own; references to profiles are checked by
    /// [`super::Catalog::validate`].
    pub fn validate(&self, ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues = Issues::new(Subject::Config);
        for (field, path) in [
            ("steam_library", &self.steam_library),
            ("profiles_dir", &self.profiles_dir),
            ("script_dir", &self.script_dir),
        ] {
            if let Some(path) = path {
                issues.absolute(field, path, ctx.home);
            }
        }
        if let Some(profile) = &self.default_profile {
            issues.name(profile);
        }
        if let Some(proxy) = &self.proxy
            && !proxy.contains("://")
        {
            issues.error(format!(
                "`proxy` must be a URL such as `http://host:8080` or `socks5://host:1080`, got `{proxy}`"
            ));
        }
        issues.finish()
    }
}
