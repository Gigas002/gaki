//! Validation findings about gaki's config files. The finding, its collector,
//! the name rule and `~` expansion are libgameprofiles', so profile files mean
//! the same to gaki, lootrs and sruman; gaki adds its own subjects
//! (`config.toml`, Steam games by app id).

use std::{fmt, path::PathBuf};

pub use libgameprofiles::{Severity, expand_tilde, validate_name};

/// A finding about one of gaki's files.
pub type Issue = libgameprofiles::Issue<Subject>;

/// Collects [`Issue`]s about one subject.
pub(super) type Issues = libgameprofiles::Issues<Subject>;

/// What validation needs from the environment.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    /// Expands a leading `~` in paths.
    pub home: &'a std::path::Path,
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

/// libgameprofiles' subjects. Its own checks only report profiles and files;
/// a game there is one by name, which gaki's `games.toml` calls an alias.
impl From<libgameprofiles::Subject> for Subject {
    fn from(subject: libgameprofiles::Subject) -> Self {
        match subject {
            libgameprofiles::Subject::Profile(name) => Self::Profile(name),
            libgameprofiles::Subject::File(path) => Self::File(path),
            libgameprofiles::Subject::Game(name) => Self::Game {
                app_id: 0,
                name: Some(name),
            },
        }
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
