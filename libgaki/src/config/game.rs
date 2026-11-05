//! `games.toml`: per-game overrides. A game only needs an entry when it
//! overrides something.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    Platform,
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
    /// A script the launch is wrapped in; it gets the game command as its
    /// arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl Game {
    pub fn new(app_id: u32) -> Self {
        Self {
            app_id,
            name: None,
            profile: None,
            platform: None,
            args: Vec::new(),
            script: None,
            env: BTreeMap::new(),
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
    pub fn validate(&self, ctx: &ValidationContext) -> Vec<Issue> {
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
        if let Some(script) = &self.script {
            issues.absolute("script", script, ctx.home);
        }
        issues.env(&self.env);
        issues.finish()
    }
}
