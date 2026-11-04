//! Steam Workshop: browsing, subscriptions, installs, ratings and comments.

use crate::core::models::{
    DownloadProgress, DownloadState, WorkshopItem, WorkshopItemKind, WorkshopQueryPage,
};
use crate::steam_client::SteamClient;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::Receiver;

/// Browse order, mapped to Steam's `EPublishedFileQueryType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Trend,
    Popular,
    Recent,
    Updated,
    Subscriptions,
    /// Text-search relevance.
    Text,
}

impl Sort {
    fn query_type(self) -> u32 {
        match self {
            Sort::Trend => 3,
            Sort::Popular => 0,
            Sort::Recent => 1,
            Sort::Updated => 21,
            Sort::Subscriptions => 9,
            Sort::Text => 12,
        }
    }
}

/// Lower-case name of an entry kind.
pub fn kind_name(kind: WorkshopItemKind) -> &'static str {
    match kind {
        WorkshopItemKind::Collection => "collection",
        WorkshopItemKind::Item => "item",
    }
}

/// A Workshop browse query.
#[derive(Debug, Clone)]
pub struct Query {
    pub app_id: u32,
    pub search: Option<String>,
    pub sort: Sort,
    /// `"*"` for the first page, else a previous page's `next_cursor`.
    pub cursor: String,
    /// 1..=100 per page.
    pub count: u32,
    /// Items must carry all of these tags.
    pub tags: Vec<String>,
}

/// One page of a game's Workshop.
pub async fn browse(client: &SteamClient, query: &Query) -> Result<WorkshopQueryPage> {
    client
        .query_workshop_files(
            query.app_id,
            query.search.as_deref(),
            query.sort.query_type(),
            &query.cursor,
            query.count,
            &query.tags,
        )
        .await
}

/// Metadata for published files.
pub async fn details(client: &SteamClient, ids: &[u64]) -> Result<Vec<WorkshopItem>> {
    client.fetch_published_file_details(ids).await
}

/// The items the account is subscribed to for a game.
pub async fn subscribed(client: &SteamClient, app_id: u32) -> Result<Vec<WorkshopItem>> {
    let ids = client.fetch_subscribed_items(app_id).await?;
    details(client, &ids).await
}

/// Metadata for `ids`, with collections expanded to their items unless
/// `recurse` is off.
pub async fn resolve(
    client: &SteamClient,
    ids: Vec<u64>,
    recurse: bool,
) -> Result<Vec<WorkshopItem>> {
    let ids = if recurse {
        client.expand_collections(&ids).await?
    } else {
        ids
    };
    details(client, &ids).await
}

/// Download an item and register it with the game.
pub async fn install<Fut: Future<Output = Result<bool>>>(
    client: &SteamClient,
    item: &WorkshopItem,
    drive: impl FnOnce(Receiver<DownloadProgress>) -> Fut,
) -> Result<()> {
    let state = Arc::new(RwLock::new(DownloadState::default()));
    let rx = client
        .install_workshop_item(item, state)
        .await
        .with_context(|| format!("failed to start install for Workshop item {}", item.id))?;
    drive(rx).await?;
    Ok(())
}

/// Remove an installed item's files.
pub async fn uninstall(client: &SteamClient, item: &WorkshopItem) -> Result<()> {
    client
        .uninstall_workshop_item(item.id, item.app_id)
        .await
        .with_context(|| format!("failed to uninstall Workshop item {}", item.id))
}

pub async fn subscribe(client: &SteamClient, item: &WorkshopItem) -> Result<()> {
    client
        .subscribe_published_file(item.id, item.app_id)
        .await
        .with_context(|| format!("failed to subscribe to Workshop item {}", item.id))
}

pub async fn unsubscribe(client: &SteamClient, item: &WorkshopItem) -> Result<()> {
    client
        .unsubscribe_published_file(item.id, item.app_id)
        .await
        .with_context(|| format!("failed to unsubscribe from Workshop item {}", item.id))
}

/// An installed or subscribed item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStatus {
    pub id: u64,
    /// Empty when Steam returned no details.
    pub title: String,
    pub installed: bool,
    pub subscribed: bool,
    /// Installed, but Steam has newer content.
    pub update_available: bool,
}

/// Installed and subscribed items for a game, with update detection. The
/// network parts are best-effort, so offline this still lists what is
/// installed.
pub async fn status(client: &SteamClient, app_id: u32) -> Result<Vec<ItemStatus>> {
    let installed = client.read_installed_workshop(app_id).await?;
    let subscribed = client
        .fetch_subscribed_items(app_id)
        .await
        .unwrap_or_default();
    let installed: Vec<(u64, u64)> = installed.iter().map(|i| (i.id, i.manifest_id)).collect();
    let ids = union_ids(&installed, &subscribed);
    let details = details(client, &ids).await.unwrap_or_default();
    Ok(status_rows(&ids, &installed, &subscribed, &details))
}

/// Installed ids, then subscribed ones not installed.
fn union_ids(installed: &[(u64, u64)], subscribed: &[u64]) -> Vec<u64> {
    let mut ids: Vec<u64> = installed.iter().map(|(id, _)| *id).collect();
    for &s in subscribed {
        if !ids.contains(&s) {
            ids.push(s);
        }
    }
    ids
}

/// `installed` holds `(id, installed manifest)` pairs.
fn status_rows(
    ids: &[u64],
    installed: &[(u64, u64)],
    subscribed: &[u64],
    details: &[WorkshopItem],
) -> Vec<ItemStatus> {
    let detail_by_id: HashMap<u64, &WorkshopItem> = details.iter().map(|d| (d.id, d)).collect();
    let manifest_by_id: HashMap<u64, u64> = installed.iter().copied().collect();
    ids.iter()
        .map(|&id| {
            let detail = detail_by_id.get(&id);
            let current = detail.map(|d| d.hcontent_file);
            let update_available = match (manifest_by_id.get(&id), current) {
                (Some(&have), Some(cur)) => cur != 0 && cur != have,
                _ => false,
            };
            ItemStatus {
                id,
                title: detail.map(|d| d.title.clone()).unwrap_or_default(),
                installed: manifest_by_id.contains_key(&id),
                subscribed: subscribed.contains(&id),
                update_available,
            }
        })
        .collect()
}

/// Thumbs-up (`up`) or thumbs-down an item.
pub async fn rate(client: &SteamClient, id: u64, up: bool) -> Result<()> {
    client.vote_workshop_item(id, up).await
}

/// A page of an item's comments.
pub async fn comments(
    client: &SteamClient,
    id: u64,
    start: i32,
    count: i32,
) -> Result<Vec<crate::core::models::WorkshopComment>> {
    client.workshop_comments(id, start, count).await
}

/// Post a comment; returns its id.
pub async fn post_comment(client: &SteamClient, id: u64, text: &str) -> Result<u64> {
    client.post_workshop_comment(id, text).await
}

#[cfg(test)]
mod tests;
