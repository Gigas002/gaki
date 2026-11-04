//! Store category names via `StoreBrowse.GetStoreCategories`.
use super::*;
use crate::core::config::{load_name_cache, save_name_cache};
use steam_vent_proto::steammessages_storebrowse_steamclient::{
    CStoreBrowse_GetStoreCategories_Request, CStoreBrowse_GetStoreCategories_Response,
};

/// Name-cache file for category names.
const CATEGORY_NAMES: &str = "category_names";

impl SteamClient {
    /// Category id → display name, disk-cached.
    pub async fn store_category_names(&self) -> Result<HashMap<u32, String>> {
        let cached = load_name_cache(CATEGORY_NAMES).await;
        if !cached.is_empty() {
            return Ok(cached);
        }
        let connection = self.require_connection()?;
        let mut request = CStoreBrowse_GetStoreCategories_Request::new();
        request.set_language(crate::core::STEAM_LANGUAGE.to_string());
        let response: CStoreBrowse_GetStoreCategories_Response = connection
            .service_method(request)
            .await
            .context("failed calling StoreBrowse.GetStoreCategories")?;
        let names: HashMap<u32, String> = response
            .categories
            .iter()
            .filter(|c| c.categoryid() != 0 && !c.display_name().is_empty())
            .map(|c| (c.categoryid(), c.display_name().to_string()))
            .collect();
        if !names.is_empty() {
            if let Err(e) = save_name_cache(CATEGORY_NAMES, &names).await {
                tracing::debug!("could not cache category names: {e:#}");
            }
        }
        Ok(names)
    }
}
