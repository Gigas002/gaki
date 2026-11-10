//! `SteamClient` methods: Steam's pre-built shader buckets.
//!
//! Split out of `steam_client.rs` for readability; the struct, shared imports
//! and free helpers live in the parent module (in scope via `use super::*`).
use super::*;

use gaki_steam_vent::{EResult, NetworkError};
use gaki_steam_vent_proto::steammessages_shader_steamclient::{
    CShader_GetBucketManifest_Request, CShader_GetBucketManifest_Response,
};

/// A pre-built shader bucket: a manifest of the depot whose id is the
/// game's app id, holding Fossilize databases or transcoded media.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderBucket {
    pub bucket_id: u64,
    pub manifest_id: u64,
    /// The bucket's size in bytes, as Steam reports it.
    pub size: u64,
}

impl SteamClient {
    /// The current bucket of `app_id` for one GPU and driver description
    /// (`Shader.GetBucketManifest`). `None` when Steam has none for it.
    pub async fn shader_bucket(
        &self,
        app_id: u32,
        gpu_desc: &str,
        driver_desc: &str,
    ) -> Result<Option<ShaderBucket>> {
        let connection = self.require_connection()?;
        let mut request = CShader_GetBucketManifest_Request::new();
        request.set_appid(app_id);
        request.set_gpu_desc(gpu_desc.to_owned());
        request.set_driver_desc(driver_desc.to_owned());
        let response: CShader_GetBucketManifest_Response =
            match connection.service_method(request).await {
                Ok(response) => response,
                Err(NetworkError::ApiError(EResult::NoMatch)) => return Ok(None),
                Err(e) => {
                    return Err(anyhow::Error::new(e))
                        .context("failed calling Shader.GetBucketManifest");
                }
            };
        Ok(Some(ShaderBucket {
            bucket_id: response.bucketid(),
            manifest_id: response.manifestid(),
            size: response.depotsize(),
        })
        .filter(|b| b.manifest_id != 0))
    }
}
