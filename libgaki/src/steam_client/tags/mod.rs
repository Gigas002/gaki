//! Store tag names: baked table, cache, then CM.
use super::*;
use crate::core::config::{load_name_cache, save_name_cache};

/// Name-cache file for tag names.
const TAG_NAMES: &str = "tag_names";
use gaki_steam_vent_proto::steammessages_store_steamclient::{
    CStore_GetLocalizedNameForTags_Request, CStore_GetLocalizedNameForTags_Response,
    CStore_GetTagList_Request, CStore_GetTagList_Response,
};

impl SteamClient {
    /// Full tag vocabulary, sorted by id.
    pub async fn fetch_tag_list(&self) -> Result<Vec<(u32, String)>> {
        let connection = self.require_connection()?;
        let mut request = CStore_GetTagList_Request::new();
        request.set_language(crate::core::STEAM_LANGUAGE.to_string());
        let response: CStore_GetTagList_Response = connection
            .service_method(request)
            .await
            .context("failed calling Store.GetTagList")?;
        let mut tags: Vec<(u32, String)> = response
            .tags
            .iter()
            .filter(|t| t.tagid() != 0 && !t.name().is_empty())
            .map(|t| (t.tagid(), t.name().to_string()))
            .collect();
        tags.sort_by_key(|(id, _)| *id);
        tags.dedup_by_key(|(id, _)| *id);
        Ok(tags)
    }

    /// Names for `ids` over the CM.
    pub async fn fetch_tag_names(&self, ids: &[u32]) -> Result<HashMap<u32, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let connection = self.require_connection()?;
        let mut request = CStore_GetLocalizedNameForTags_Request::new();
        request.set_language(crate::core::STEAM_LANGUAGE.to_string());
        request.tagids = ids.to_vec();
        let response: CStore_GetLocalizedNameForTags_Response = connection
            .service_method(request)
            .await
            .context("failed calling Store.GetLocalizedNameForTags")?;
        Ok(response
            .tags
            .iter()
            .filter(|t| t.tagid() != 0)
            .map(|t| {
                let name = if t.name().is_empty() {
                    t.english_name()
                } else {
                    t.name()
                };
                (t.tagid(), name.to_string())
            })
            .filter(|(_, name)| !name.is_empty())
            .collect())
    }

    /// Names for `ids`: baked table, disk cache, then CM.
    pub async fn resolve_tag_names(&self, ids: &[u32]) -> HashMap<u32, String> {
        let mut names: HashMap<u32, String> = ids
            .iter()
            .filter_map(|&id| tags_table::tag_name(id).map(|n| (id, n.to_string())))
            .collect();
        if names.len() == ids.len() {
            return names;
        }
        let mut cached = load_name_cache(TAG_NAMES).await;
        for &id in ids {
            if let Some(name) = cached.get(&id) {
                names.entry(id).or_insert_with(|| name.clone());
            }
        }
        let missing: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| !names.contains_key(id))
            .collect();
        if missing.is_empty() {
            return names;
        }
        match self.fetch_tag_names(&missing).await {
            Ok(fetched) if !fetched.is_empty() => {
                cached.extend(fetched.iter().map(|(k, v)| (*k, v.clone())));
                if let Err(e) = save_name_cache(TAG_NAMES, &cached).await {
                    tracing::debug!("could not cache tag names: {e:#}");
                }
                names.extend(fetched);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("tag names unavailable from Steam: {e:#}"),
        }
        names
    }
}

#[cfg(test)]
mod tests;
