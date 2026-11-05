//! The Community Market, inventory and wallet. Account data needs a web
//! session ([`crate::ops::session::web_session`]); price lookups and search
//! need no login. The inventory, wallet and listings are sensitive: they
//! take a [`SensitiveGrant`] (see `reauth_sensitive`).

use crate::ops::auth::SensitiveGrant;
use crate::steam_client::{
    InventoryItem, MarketPrice, MarketSearchResult, MyMarketState, WalletBalance,
};
use crate::web::web_session::WebSession;
use anyhow::Result;

/// The account's inventory for an app (`context` 2 holds most games' items).
pub async fn inventory(
    web: &WebSession,
    app_id: u32,
    context: u32,
    _grant: SensitiveGrant,
) -> Result<Vec<InventoryItem>> {
    crate::steam_client::inventory_via(web, app_id, context).await
}

/// The Steam Wallet balance.
pub async fn wallet(web: &WebSession, _grant: SensitiveGrant) -> Result<WalletBalance> {
    crate::steam_client::wallet_via(web).await
}

/// The account's active listings and open buy orders.
pub async fn listings(web: &WebSession, _grant: SensitiveGrant) -> Result<MyMarketState> {
    crate::steam_client::my_listings_via(web).await
}

/// An item's current price, in Steam currency `currency`.
pub async fn price(app_id: u32, market_hash_name: &str, currency: u32) -> Result<MarketPrice> {
    crate::steam_client::market_price(app_id, market_hash_name, currency).await
}

/// Search the market. Returns the total match count and one page.
pub async fn search(
    query: Option<&str>,
    app_id: Option<u32>,
    count: u32,
) -> Result<(u32, Vec<MarketSearchResult>)> {
    crate::steam_client::market_search(query, app_id, count).await
}
