//! Library collections: a local store edited offline, then pulled from,
//! pushed to or synced with the Steam account.

use crate::library::collections::{self, Collection, CollectionsStore};
use crate::steam_client::SteamClient;
use anyhow::Result;

/// What kind of collection it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Made by Steam (favorites, hidden).
    BuiltIn,
    /// Filter-based; Steam computes the members.
    Dynamic,
    /// An explicit member list.
    Static,
}

impl Kind {
    pub fn of(c: &Collection) -> Self {
        if c.is_builtin() {
            Kind::BuiltIn
        } else if c.is_dynamic() {
            Kind::Dynamic
        } else {
            Kind::Static
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::BuiltIn => "built-in",
            Kind::Dynamic => "dynamic",
            Kind::Static => "static",
        }
    }
}

/// One collection, as listed.
#[derive(Debug, Clone)]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// Members; `None` for a dynamic collection.
    pub app_ids: Option<Vec<u32>>,
}

impl Summary {
    fn of(c: &Collection) -> Self {
        let kind = Kind::of(c);
        let app_ids = (kind != Kind::Dynamic).then(|| {
            c.added
                .iter()
                .copied()
                .filter(|a| !c.removed.contains(a))
                .collect()
        });
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            kind,
            app_ids,
        }
    }

    /// Member count (0 for a dynamic collection).
    pub fn count(&self) -> usize {
        self.app_ids.as_ref().map_or(0, Vec::len)
    }
}

/// The local collections (not deleted) and the store's namespace version.
pub fn list() -> Result<(u64, Vec<Summary>)> {
    let store = CollectionsStore::load()?;
    let visible = store
        .collections
        .iter()
        .filter(|c| !c.deleted)
        .map(Summary::of)
        .collect();
    Ok((store.namespace_version, visible))
}

/// One collection by name or id.
pub fn show(name_or_id: &str) -> Result<Summary> {
    let store = CollectionsStore::load()?;
    Ok(Summary::of(store.resolve(name_or_id)?))
}

/// Create a static collection; returns its id.
pub fn create(name: &str) -> Result<String> {
    collections::edit(|s| s.create(name))
}

/// Mark a collection for deletion on the next push.
pub fn delete(name_or_id: &str) -> Result<()> {
    collections::edit(|s| s.delete(name_or_id))
}

pub fn rename(name_or_id: &str, new_name: &str) -> Result<()> {
    collections::edit(|s| s.rename(name_or_id, new_name))
}

pub fn add(name_or_id: &str, app_ids: &[u32]) -> Result<()> {
    collections::edit(|s| s.add(name_or_id, app_ids))
}

pub fn remove(name_or_id: &str, app_ids: &[u32]) -> Result<()> {
    collections::edit(|s| s.remove(name_or_id, app_ids))
}

/// Collections stored locally (including ones marked deleted).
pub fn local_count() -> Result<usize> {
    Ok(CollectionsStore::load()?.collections.len())
}

/// The store after a pull, push or sync.
#[derive(Debug, Clone, Copy)]
pub struct Synced {
    pub namespace_version: u64,
    /// Collections now stored locally (a pull counts only live ones).
    pub count: usize,
}

/// Download Steam's collections and merge them into the local store.
pub async fn pull(client: &SteamClient) -> Result<Synced> {
    let store = collections::pull(client).await?;
    Ok(Synced {
        namespace_version: store.namespace_version,
        count: store.collections.iter().filter(|c| !c.deleted).count(),
    })
}

/// Upload the local collections (changes the Steam account).
pub async fn push(client: &SteamClient) -> Result<Synced> {
    let store = collections::push(client).await?;
    Ok(Synced {
        namespace_version: store.namespace_version,
        count: store.collections.len(),
    })
}

/// Pull, then push (changes the Steam account).
pub async fn sync(client: &SteamClient) -> Result<Synced> {
    let store = collections::sync(client).await?;
    Ok(Synced {
        namespace_version: store.namespace_version,
        count: store.collections.len(),
    })
}

/// One of Steam's built-in collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltIn {
    Favorite,
    Hidden,
}

impl BuiltIn {
    pub fn id(self) -> &'static str {
        match self {
            BuiltIn::Favorite => collections::FAVORITE,
            BuiltIn::Hidden => collections::HIDDEN,
        }
    }
}

/// The members of a built-in collection; none if the store has none yet.
pub fn builtin_members(which: BuiltIn) -> Result<Vec<u32>> {
    let store = CollectionsStore::load()?;
    Ok(store
        .collections
        .iter()
        .find(|c| !c.deleted && c.id == which.id())
        .map(|c| Summary::of(c).app_ids.unwrap_or_default())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests;
