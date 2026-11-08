//! The Community Market, inventory and wallet. Account data needs a web
//! session ([`crate::ops::session::web_session`]); price lookups and search
//! need no login. The inventory, wallet and listings are sensitive: they
//! take a [`SensitiveGrant`] (see `reauth_sensitive`).
//!
//! Answers are reused for a while and requests are spaced out ([`pace`]):
//! a price or search for [`pace::PUBLIC_TTL`], account data for
//! [`pace::ACCOUNT_TTL`], at most one request per [`pace::MIN_INTERVAL`].

pub mod pace;

use std::sync::LazyLock;

use crate::ops::auth::SensitiveGrant;
use crate::steam_client::{
    InventoryItem, MarketPrice, MarketSearchResult, MyMarketState, WalletBalance,
};
use crate::web::web_session::WebSession;
use anyhow::Result;
use pace::{ACCOUNT_TTL, Cache, PUBLIC_TTL, cached};

static INVENTORY: LazyLock<Cache<(u64, u32, u32), Vec<InventoryItem>>> =
    LazyLock::new(|| Cache::new(ACCOUNT_TTL));
static WALLET: LazyLock<Cache<u64, WalletBalance>> = LazyLock::new(|| Cache::new(ACCOUNT_TTL));
static LISTINGS: LazyLock<Cache<u64, MyMarketState>> = LazyLock::new(|| Cache::new(ACCOUNT_TTL));
static PRICES: LazyLock<Cache<(u32, String, u32), MarketPrice>> =
    LazyLock::new(|| Cache::new(PUBLIC_TTL));
type SearchKey = (Option<String>, Option<u32>, u32);
static SEARCHES: LazyLock<Cache<SearchKey, (u32, Vec<MarketSearchResult>)>> =
    LazyLock::new(|| Cache::new(PUBLIC_TTL));

/// The account's inventory for an app (`context` 2 holds most games' items).
pub async fn inventory(
    web: &WebSession,
    app_id: u32,
    context: u32,
    _grant: SensitiveGrant,
) -> Result<Vec<InventoryItem>> {
    let key = (web.steam_id(), app_id, context);
    cached(
        &INVENTORY,
        key,
        crate::steam_client::inventory_via(web, app_id, context),
    )
    .await
}

/// The Steam Wallet balance.
pub async fn wallet(web: &WebSession, _grant: SensitiveGrant) -> Result<WalletBalance> {
    cached(
        &WALLET,
        web.steam_id(),
        crate::steam_client::wallet_via(web),
    )
    .await
}

/// The account's active listings and open buy orders.
pub async fn listings(web: &WebSession, _grant: SensitiveGrant) -> Result<MyMarketState> {
    cached(
        &LISTINGS,
        web.steam_id(),
        crate::steam_client::my_listings_via(web),
    )
    .await
}

/// An item's current price, in Steam currency `currency`.
pub async fn price(app_id: u32, market_hash_name: &str, currency: u32) -> Result<MarketPrice> {
    let key = (app_id, market_hash_name.to_string(), currency);
    let fetch = crate::steam_client::market_price(app_id, market_hash_name, currency);
    cached(&PRICES, key, fetch).await
}

/// Search the market. Returns the total match count and one page.
pub async fn search(
    query: Option<&str>,
    app_id: Option<u32>,
    count: u32,
) -> Result<(u32, Vec<MarketSearchResult>)> {
    let key = (query.map(str::to_string), app_id, count);
    cached(
        &SEARCHES,
        key,
        crate::steam_client::market_search(query, app_id, count),
    )
    .await
}

#[cfg(test)]
mod tests;
