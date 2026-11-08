//! Favourite and hidden games. With the `collections` feature these are
//! Steam's own built-in collections: a change edits the local collections
//! store at once and is synced to Steam in the background. Without it they
//! are the `favorite_games` and `hidden_games` lists in `tui/config.toml`.
//!
//! The library list asks about every game several times per keypress, so
//! the members are cached here and reloaded only after a change or a sync.

use std::collections::HashSet;
use std::sync::RwLock;

use crate::config::Config;
use crate::utils::error::TuiError;

#[derive(Default)]
struct Members {
    favourite: HashSet<i32>,
    hidden: HashSet<i32>,
}

static MEMBERS: RwLock<Option<Members>> = RwLock::new(None);

fn with<T>(f: impl Fn(&Members) -> T) -> T {
    if let Some(members) = &*MEMBERS.read().unwrap_or_else(|e| e.into_inner()) {
        return f(members);
    }
    reload();
    f(MEMBERS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .unwrap_or(&Members::default()))
}

/// Whether `app_id` is a favourite.
pub fn is_favourite(app_id: i32) -> bool {
    with(|m| m.favourite.contains(&app_id))
}

/// Whether `app_id` is hidden from the library.
pub fn is_hidden(app_id: i32) -> bool {
    with(|m| m.hidden.contains(&app_id))
}

/// Read the members again from where they are kept. A read that fails is
/// logged and leaves no members, so the list still works.
pub fn reload() {
    let members = load().unwrap_or_else(|e| {
        tracing::warn!("reading favourite and hidden games: {e}");
        Members::default()
    });
    *MEMBERS.write().unwrap_or_else(|e| e.into_inner()) = Some(members);
}

/// Make `app_id` a favourite, or stop it being one. `config` is the
/// caller's copy of the settings, which keeps the lists without the
/// `collections` feature.
pub fn set_favourite(config: &mut Config, app_id: i32, on: bool) -> Result<(), TuiError> {
    store::set(config, Mark::Favourite, app_id, on)?;
    reload();
    Ok(())
}

/// Hide `app_id` from the library.
pub fn hide(config: &mut Config, app_id: i32) -> Result<(), TuiError> {
    store::set(config, Mark::Hidden, app_id, true)?;
    reload();
    Ok(())
}

/// Why the last background sync with Steam failed, once.
pub fn take_sync_error() -> Option<String> {
    store::take_sync_error()
}

#[derive(Clone, Copy)]
enum Mark {
    Favourite,
    Hidden,
}

#[cfg(feature = "collections")]
fn load() -> Result<Members, TuiError> {
    use libgaki::ops::collections::{BuiltIn, builtin_members};
    let read = |which| -> Result<HashSet<i32>, TuiError> {
        let ids = builtin_members(which).map_err(|e| TuiError::Problem(format!("{e:#}")))?;
        Ok(ids
            .into_iter()
            .filter_map(|a| i32::try_from(a).ok())
            .collect())
    };
    Ok(Members {
        favourite: read(BuiltIn::Favorite)?,
        hidden: read(BuiltIn::Hidden)?,
    })
}

#[cfg(not(feature = "collections"))]
fn load() -> Result<Members, TuiError> {
    let config = Config::cached();
    Ok(Members {
        favourite: config.favorite_games.iter().copied().collect(),
        hidden: config.hidden_games.iter().copied().collect(),
    })
}

/// Move the `favorite_games` and `hidden_games` lists of an older
/// `tui/config.toml` into Steam's collections, once, then drop them from
/// the file. Without the `collections` feature the lists stay where they
/// are.
pub fn migrate(config: &mut Config) -> Result<(), TuiError> {
    #[cfg(feature = "collections")]
    if !config.favorite_games.is_empty() || !config.hidden_games.is_empty() {
        use libgaki::ops::collections::{BuiltIn, add};
        let ids = |list: &[i32]| -> Vec<u32> {
            list.iter().filter_map(|&a| u32::try_from(a).ok()).collect()
        };
        let problem = |e: anyhow::Error| TuiError::Problem(format!("{e:#}"));
        add(BuiltIn::Favorite.id(), &ids(&config.favorite_games)).map_err(problem)?;
        add(BuiltIn::Hidden.id(), &ids(&config.hidden_games)).map_err(problem)?;
        tracing::info!(
            favourites = config.favorite_games.len(),
            hidden = config.hidden_games.len(),
            "moved favourite and hidden games into Steam's collections"
        );
        config.favorite_games.clear();
        config.hidden_games.clear();
        config.save()?;
        store::request_sync();
    }
    #[cfg(not(feature = "collections"))]
    let _ = config;
    reload();
    Ok(())
}

/// Fetch Steam's collections into the local store (after the library
/// loads), so marks made in the Steam client show up.
pub fn pull() {
    store::pull();
}

#[cfg(feature = "collections")]
mod store {
    use std::sync::mpsc::{self, Sender};
    use std::sync::{Mutex, OnceLock};

    use super::Mark;
    use crate::app::steam;
    use crate::config::Config;
    use crate::utils::error::TuiError;
    use libgaki::ops::collections::BuiltIn;

    static SYNC_ERROR: Mutex<Option<String>> = Mutex::new(None);

    pub(super) fn set(_: &mut Config, mark: Mark, app_id: i32, on: bool) -> Result<(), TuiError> {
        let id = match mark {
            Mark::Favourite => BuiltIn::Favorite.id(),
            Mark::Hidden => BuiltIn::Hidden.id(),
        };
        if on {
            steam::collection_add(id, &[app_id])?;
        } else {
            steam::collection_remove(id, &[app_id])?;
        }
        request_sync();
        Ok(())
    }

    pub(super) fn take_sync_error() -> Option<String> {
        SYNC_ERROR.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    enum Job {
        Pull,
        Sync,
    }

    /// One thread runs the jobs in order; a burst of toggles becomes one
    /// sync. The store itself is safe to edit while a sync runs.
    fn send(job: Job) {
        static TX: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
        let tx = TX.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            std::thread::spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut sync = matches!(first, Job::Sync);
                    while let Ok(next) = rx.try_recv() {
                        sync |= matches!(next, Job::Sync);
                    }
                    let result = if sync {
                        steam::collection_sync()
                    } else {
                        steam::collections_pull()
                    };
                    match result {
                        Ok(()) => super::reload(),
                        Err(e) => {
                            tracing::warn!(error = %e, "collections sync failed");
                            if sync {
                                *SYNC_ERROR.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(format!("Saved here; not yet sent to Steam: {e}"));
                            }
                        }
                    }
                }
            });
            Mutex::new(tx)
        });
        let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(job);
    }

    pub(super) fn request_sync() {
        send(Job::Sync);
    }

    pub(super) fn pull() {
        send(Job::Pull);
    }
}

#[cfg(not(feature = "collections"))]
mod store {
    use super::Mark;
    use crate::config::Config;
    use crate::utils::error::TuiError;

    pub(super) fn set(
        config: &mut Config,
        mark: Mark,
        app_id: i32,
        on: bool,
    ) -> Result<(), TuiError> {
        let list = match mark {
            Mark::Favourite => &mut config.favorite_games,
            Mark::Hidden => &mut config.hidden_games,
        };
        list.retain(|&x| x != app_id);
        if on {
            list.push(app_id);
        }
        config.save()
    }

    pub(super) fn take_sync_error() -> Option<String> {
        None
    }

    pub(super) fn pull() {}
}
