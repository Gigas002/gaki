//! Validation findings, and the checks the config files share.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

/// What validation needs from the environment.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    /// Expands a leading `~` in paths.
    pub home: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// What an [`Issue`] is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// `config.toml`.
    Config,
    /// One `[[game]]` in `games.toml`, by app id and alias.
    Game {
        app_id: u32,
        name: Option<String>,
    },
    Profile(String),
    /// A file that could not be loaded at all.
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub subject: Subject,
    pub message: String,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config => f.write_str("config.toml"),
            Self::Game {
                app_id,
                name: Some(name),
            } => write!(f, "game {app_id} (`{name}`)"),
            Self::Game { app_id, name: None } => write!(f, "game {app_id}"),
            Self::Profile(name) => write!(f, "profile `{name}`"),
            Self::File(path) => write!(f, "{}", path.display()),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.severity, self.subject, self.message)
    }
}

/// Checks a profile name or game alias: `[A-Za-z0-9._-]+`, not `.`/`..`, not
/// starting with `-`. The same rule as lootrs, so profile files are shared.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name must not be empty".into());
    }
    if name == "." || name == ".." || name.starts_with('-') {
        return Err(format!("name `{name}` is not allowed"));
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
    {
        return Err(format!(
            "name `{name}` contains `{c}`; use only letters, digits, `.`, `_` and `-`"
        ));
    }
    Ok(())
}

/// `path` with a leading `~` replaced by `home`.
pub fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

/// Collects issues for one subject.
pub(super) struct Issues {
    subject: Subject,
    list: Vec<Issue>,
}

impl Issues {
    pub(super) fn new(subject: Subject) -> Self {
        Self {
            subject,
            list: Vec::new(),
        }
    }

    fn push(&mut self, severity: Severity, message: impl Into<String>) {
        self.list.push(Issue {
            severity,
            subject: self.subject.clone(),
            message: message.into(),
        });
    }

    pub(super) fn error(&mut self, message: impl Into<String>) {
        self.push(Severity::Error, message);
    }

    pub(super) fn warn(&mut self, message: impl Into<String>) {
        self.push(Severity::Warning, message);
    }

    pub(super) fn name(&mut self, name: &str) {
        if let Err(message) = validate_name(name) {
            self.error(message);
        }
    }

    /// Errors unless `path` is absolute after `~` expansion; returns the
    /// expanded path.
    pub(super) fn absolute(&mut self, field: &str, path: &Path, home: &Path) -> Option<PathBuf> {
        let expanded = expand_tilde(path, home);
        if expanded.is_absolute() {
            Some(expanded)
        } else {
            self.error(format!(
                "`{field}` must be an absolute path (or start with `~`), got `{}`",
                path.display()
            ));
            None
        }
    }

    pub(super) fn env(&mut self, env: &BTreeMap<String, String>) {
        for key in env.keys() {
            if key.is_empty() || key.contains(['=', '\0']) {
                self.error(format!("`env` key `{key}` is not a valid variable name"));
            }
        }
    }

    pub(super) fn finish(self) -> Vec<Issue> {
        self.list
    }
}
