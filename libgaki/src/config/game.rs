//! `games.toml`: per-game overrides. A game only needs an entry when it
//! overrides something.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    Platform, SteamIntegration,
    issue::{Issue, Issues, Subject, ValidationContext},
};

/// The whole file: a list of `[[game]]` tables.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamesFile {
    #[serde(default, rename = "game", skip_serializing_if = "Vec::is_empty")]
    pub games: Vec<Game>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Game {
    pub app_id: u32,
    /// An alias usable everywhere an app id is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The profile to run with, instead of `default_profile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    /// Arguments passed to the game.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Commands the launch is wrapped in, inside the profile's: each gets the
    /// rest of the command as its arguments, so a script that ends in
    /// `exec "$@"` works.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrappers: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Steam integration, instead of the profile's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam: Option<SteamIntegration>,
}

impl Game {
    pub fn new(app_id: u32) -> Self {
        Self {
            app_id,
            name: None,
            profile: None,
            platform: None,
            args: Vec::new(),
            wrappers: Vec::new(),
            env: BTreeMap::new(),
            steam: None,
        }
    }

    pub(super) fn subject(&self) -> Subject {
        Subject::Game {
            app_id: self.app_id,
            name: self.name.clone(),
        }
    }

    /// Checks this entry on its own; duplicates and profile references are
    /// checked by [`super::Catalog::validate`].
    pub fn validate(&self, _ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues = Issues::new(self.subject());
        if self.app_id == 0 {
            issues.error("`app_id` must not be 0");
        }
        if let Some(name) = &self.name {
            issues.name(name);
            if name.bytes().all(|b| b.is_ascii_digit()) {
                issues.error(format!(
                    "`name` `{name}` is all digits, so it would read as an app id"
                ));
            }
        }
        if let Some(profile) = &self.profile {
            issues.name(profile);
        }
        for wrapper in &self.wrappers {
            if wrapper.trim().is_empty() {
                issues.error("`wrappers` must not contain empty commands");
            }
        }
        issues.env(&self.env);
        issues.finish()
    }
}
