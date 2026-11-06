//! The Steam store: prices, search, deals, wishlists, news and reviews.

use crate::core::error::{ErrorKind, TypedError};
use crate::steam_client::{
    DealsScope, SteamClient, StoreAppInfo, StoreEvent, StorePurchaseOption, WishlistEntry,
    unix_to_ymd,
};
use anyhow::Result;
use std::time::Duration;

pub use crate::ops::info::{DEFAULT_COUNTRY, resolve_country};

/// Parse `--compare`-style region lists (`["US,DE", "jp"]`) into normalized,
/// de-duplicated country codes.
pub fn parse_regions(lists: &[String]) -> Result<Vec<String>> {
    let mut regions: Vec<String> = Vec::new();
    for raw in lists.iter().flat_map(|s| s.split(',')) {
        if raw.trim().is_empty() {
            continue;
        }
        let cc = crate::core::utils::normalize_country(raw).ok_or_else(|| {
            TypedError::new(
                ErrorKind::InvalidInput,
                format!("invalid country code `{raw}` — use two-letter ISO codes like US,DE,JP"),
            )
        })?;
        if !regions.contains(&cc) {
            regions.push(cc);
        }
    }
    Ok(regions)
}

/// Spacing between per-region store calls.
const REGION_SPACING: Duration = Duration::from_millis(150);

/// One region's price for an app.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PriceQuote {
    pub country: String,
    pub available: bool,
    pub is_free: bool,
    pub price: Option<String>,
    pub price_cents: Option<i64>,
    pub original_price: Option<String>,
    pub original_price_cents: Option<i64>,
    pub discount_pct: i32,
    pub discount_end: Option<u64>,
    pub discount_end_date: Option<String>,
    pub purchase_options: Vec<StorePurchaseOption>,
    /// Why the region could not be queried.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PriceQuote {
    fn from_info(info: &StoreAppInfo) -> Self {
        Self {
            country: info.country.clone(),
            available: !info.region_locked && (info.is_free || info.price.is_some()),
            is_free: info.is_free,
            price: info.price.clone(),
            price_cents: info.price_cents,
            original_price: info.original_price.clone(),
            original_price_cents: info.original_price_cents,
            discount_pct: info.discount_pct,
            discount_end: info.discount_end,
            discount_end_date: info.discount_end.map(|t| unix_to_ymd(t as i64)),
            purchase_options: info.purchase_options.clone(),
            error: None,
        }
    }

    fn unavailable(country: &str, error: Option<String>) -> Self {
        Self {
            country: country.to_string(),
            available: false,
            is_free: false,
            price: None,
            price_cents: None,
            original_price: None,
            original_price_cents: None,
            discount_pct: 0,
            discount_end: None,
            discount_end_date: None,
            purchase_options: Vec::new(),
            error,
        }
    }
}

/// An app's prices across regions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Prices {
    pub name: String,
    pub regions: Vec<PriceQuote>,
}

/// Quote an app in each region. A failed region is reported in its quote;
/// the call fails (`NotFound`) only when no region knows the app.
pub async fn prices(client: &SteamClient, app_id: u32, regions: &[String]) -> Result<Prices> {
    let mut name: Option<String> = None;
    let mut quotes = Vec::with_capacity(regions.len());
    for (i, region) in regions.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(REGION_SPACING).await;
        }
        let quote = match client.fetch_store_apps(&[app_id], region).await {
            Ok(apps) => match apps.into_iter().find(|a| a.app_id == app_id) {
                Some(info) => {
                    if name.is_none() && !info.name.is_empty() {
                        name = Some(info.name.clone());
                    }
                    PriceQuote::from_info(&info)
                }
                None => PriceQuote::unavailable(region, None),
            },
            Err(e) => PriceQuote::unavailable(region, Some(format!("{e:#}"))),
        };
        quotes.push(quote);
    }
    let name = name.ok_or_else(|| no_store_info(app_id))?;
    Ok(Prices {
        name,
        regions: quotes,
    })
}

fn no_store_info(app_id: u32) -> anyhow::Error {
    TypedError::new(
        ErrorKind::NotFound,
        format!("no store information available for app {app_id}"),
    )
    .into()
}

/// Store records for `ids`, in the order of `ids`.
pub async fn records(
    client: &SteamClient,
    ids: &[u32],
    country: &str,
) -> Result<Vec<StoreAppInfo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut records = client.fetch_store_apps(ids, country).await?;
    sort_by_request(&mut records, ids);
    Ok(records)
}

fn sort_by_request(records: &mut [StoreAppInfo], ids: &[u32]) {
    let position = |id: u32| ids.iter().position(|&x| x == id).unwrap_or(usize::MAX);
    records.sort_by_key(|r| position(r.app_id));
}

/// One page of store results.
#[derive(Debug, Clone)]
pub struct Listing {
    /// Matches in total, beyond this page.
    pub total: i32,
    /// Spelling suggestions (search only).
    pub suggestions: Vec<String>,
    pub records: Vec<StoreAppInfo>,
}

async fn listing(
    client: &SteamClient,
    page: crate::steam_client::QueryPage,
    country: &str,
) -> Result<Listing> {
    let records = records(client, &page.app_ids, country).await?;
    Ok(Listing {
        total: page.total,
        suggestions: page.suggestions,
        records,
    })
}

/// Search the store by title.
pub async fn search(
    client: &SteamClient,
    term: &str,
    count: u32,
    country: &str,
) -> Result<Listing> {
    let page = client.search_store(term, count, country).await?;
    listing(client, page, country).await
}

/// Which deals [`deals`] lists.
#[derive(Debug, Clone, Copy)]
pub struct DealsQuery {
    pub scope: DealsScope,
    pub min_discount: i32,
    pub start: i32,
    pub count: i32,
}

/// Discounted and top-selling games.
pub async fn deals(client: &SteamClient, query: DealsQuery, country: &str) -> Result<Listing> {
    let page = client
        .query_deals(
            query.scope,
            query.min_discount,
            query.start,
            query.count,
            country,
        )
        .await?;
    listing(client, page, country).await
}

/// Games related to `app_id`.
pub async fn similar(
    client: &SteamClient,
    app_id: u32,
    count: i32,
    country: &str,
) -> Result<Listing> {
    let page = client.similar_apps(app_id, count, country).await?;
    listing(client, page, country).await
}

/// Players currently in-game.
pub async fn players(client: &SteamClient, app_id: u32) -> Result<i32> {
    client.current_players(app_id).await
}

/// Active store sales and events.
pub async fn events(client: &SteamClient, country: &str) -> Result<Vec<StoreEvent>> {
    client.active_store_events(country).await
}

/// Announcements from an app's news feed.
pub async fn news(
    app_id: u32,
    count: u32,
    max_length: u32,
) -> Result<Vec<crate::web::discovery::NewsItem>> {
    let http = crate::core::net::steam_web_client()?;
    crate::web::discovery::fetch_news(&http, app_id, count, max_length).await
}

/// One page of user reviews.
pub async fn reviews(
    app_id: u32,
    query: &crate::web::discovery::ReviewQuery,
) -> Result<crate::web::discovery::ReviewPage> {
    let http = crate::core::net::steam_web_client()?;
    crate::web::discovery::fetch_reviews(&http, app_id, query).await
}

/// A page of a wishlist.
#[derive(Debug, Clone)]
pub struct Wishlist {
    pub steam_id: u64,
    /// Items on the whole wishlist.
    pub total: usize,
    /// The page, each item with its store record when the store has one.
    pub items: Vec<(WishlistEntry, Option<StoreAppInfo>)>,
}

/// A page of the wishlist of `steam_id`, or of the signed-in account.
///
/// Steam only serves friends' wishlists; others fail `PrivacyRestricted`.
pub async fn wishlist(
    client: &SteamClient,
    steam_id: Option<u64>,
    offset: usize,
    count: usize,
    country: &str,
) -> Result<Wishlist> {
    let own_id = client.steam_id();
    let steam_id = steam_id
        .or(own_id)
        .ok_or_else(|| TypedError::new(ErrorKind::AuthRequired, "not logged in"))?;
    let is_own = own_id == Some(steam_id);
    let entries = client.wishlist(steam_id).await.map_err(|e| {
        if crate::core::error::classify(&e).kind == ErrorKind::AccessDenied {
            TypedError::new(
                ErrorKind::PrivacyRestricted,
                format!(
                    "Steam does not expose the wishlist of {steam_id} to you (only friends' \
                     wishlists are readable)"
                ),
            )
            .into()
        } else {
            e
        }
    })?;
    if entries.is_empty() && !is_own {
        return Err(TypedError::new(
            ErrorKind::PrivacyRestricted,
            format!(
                "no wishlist items for {steam_id}: the wishlist may be private, friends-only, \
                 or empty"
            ),
        )
        .into());
    }
    let total = entries.len();
    let page: Vec<WishlistEntry> = entries.into_iter().skip(offset).take(count).collect();
    let ids: Vec<u32> = page.iter().map(|e| e.app_id).collect();
    let mut found = records(client, &ids, country).await?;
    let items = page
        .into_iter()
        .map(|e| {
            let record = found
                .iter()
                .position(|r| r.app_id == e.app_id)
                .map(|i| found.swap_remove(i));
            (e, record)
        })
        .collect();
    Ok(Wishlist {
        steam_id,
        total,
        items,
    })
}

/// Add an app to the wishlist; returns the new item count.
pub async fn wishlist_add(client: &SteamClient, app_id: u32) -> Result<u32> {
    client.wishlist_add(app_id).await
}

/// Remove an app from the wishlist; returns the new item count.
pub async fn wishlist_remove(client: &SteamClient, app_id: u32) -> Result<u32> {
    client.wishlist_remove(app_id).await
}

#[cfg(test)]
mod tests;
