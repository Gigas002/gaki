// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use depot::AppDepots;
use futures::TryStreamExt;
use gaki_steam_vent::{Connection, ConnectionTrait};
use gaki_steam_vent_proto::{
    steammessages_clientserver_2::{
        CMsgClientGetDepotDecryptionKey, CMsgClientGetDepotDecryptionKeyResponse,
    },
    steammessages_contentsystem_steamclient::CContentServerDirectory_GetManifestRequestCode_Request,
};
use inner::InnerClient;
use manifest::DepotManifest;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{error::Error, web_api, web_api::content_service::CDNServer};

pub mod depot;
pub mod depot_chunk;
pub mod inner;
pub mod manifest;
pub(crate) mod paths;

pub const MANIFEST_VERSION: usize = 5;

/// Chunk requests in flight per depot download.
pub const CONCURRENCY: usize = 16;

#[derive(Debug)]
pub struct CDNClient {
    inner: Arc<InnerClient>,
}

impl CDNClient {
    pub fn new(connection: Arc<Connection>) -> Self {
        Self {
            inner: Arc::new(InnerClient::new(connection)),
        }
    }

    pub fn with_servers(connection: Arc<Connection>, servers: Vec<CDNServer>) -> Self {
        let mut inner = InnerClient::new(connection);
        inner.set_servers(servers);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn with_server(connection: Arc<Connection>, server: CDNServer) -> Self {
        Self::with_servers(connection, vec![server])
    }

    pub async fn discover(connection: Arc<Connection>) -> Result<Self, Error> {
        let mut inner = InnerClient::new(connection);
        let mut servers =
            web_api::content_service::get_servers_for_steam_pipe(inner.cell_id()).await?;
        servers.sort_by_key(|s| s.weighted_load);
        inner.set_servers(servers);
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub async fn get_depots(&self, app_ids: Vec<u32>) -> Result<Vec<AppDepots>, Error> {
        let product_info = self.inner.get_product_info(app_ids).await?;
        let mut apps_depots: Vec<AppDepots> = Vec::new();

        for app in product_info.apps {
            let mut app_depots = AppDepots::new(app.appid());
            app_depots.vdf_parse(app.buffer())?;
            apps_depots.push(app_depots);
        }

        Ok(apps_depots)
    }

    pub async fn get_depot_decryption_key(
        &self,
        app_id: u32,
        depot_id: u32,
    ) -> Result<Option<[u8; 32]>, Error> {
        let response: CMsgClientGetDepotDecryptionKeyResponse = self
            .inner
            .connection
            .job(CMsgClientGetDepotDecryptionKey {
                depot_id: Some(depot_id),
                app_id: Some(app_id),
                ..Default::default()
            })
            .await?;
        match response.depot_encryption_key {
            Some(bytes) => <[u8; 32]>::try_from(bytes.as_slice())
                .map(Some)
                .map_err(|_| Error::Unexpected("depot key has unexpected size".to_string())),
            None => Ok(None),
        }
    }

    pub async fn get_manifest_request_code(
        &self,
        app_id: u32,
        depot_id: u32,
        manifest_id: u64,
    ) -> Result<u64, Error> {
        self.inner
            .connection
            .service_method(CContentServerDirectory_GetManifestRequestCode_Request {
                app_id: Some(app_id),
                depot_id: Some(depot_id),
                manifest_id: Some(manifest_id),
                ..Default::default()
            })
            .await?
            .manifest_request_code
            .ok_or(Error::Unexpected(
                "failed to get manifest request code".to_string(),
            ))
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "libgaki's install path passes each of these; an options struct is a later API change"
    )]
    pub async fn download_depot(
        &self,
        app_id: u32,
        depot_id: u32,
        manifest_id: u64,
        depot_key: &[u8],
        target_dir: impl AsRef<std::path::Path>,
        manifest_request_code: Option<u64>,
        verify_mode: bool,
        abort_signal: Option<Arc<AtomicBool>>,
        on_progress: Option<manifest::file::ProgressFn>,
        on_manifest: Option<Arc<dyn Fn(u64) + Send + Sync + 'static>>,
    ) -> Result<(), Error> {
        let request_code = if manifest_request_code.is_some() {
            manifest_request_code
        } else {
            self.get_manifest_request_code(app_id, depot_id, manifest_id)
                .await
                .ok()
        };

        let key_arr: [u8; 32] = depot_key
            .get(..32)
            .and_then(|key| key.try_into().ok())
            .ok_or_else(|| Error::Unexpected("depot key is shorter than 32 bytes".to_string()))?;

        let manifest = self
            .get_manifest(app_id, depot_id, manifest_id, request_code, Some(key_arr))
            .await?;

        if let Some(ref cb) = on_manifest {
            cb(manifest.original_size());
        }

        // Every entry is checked against the real (canonical) directory.
        let target_dir = target_dir.as_ref();
        std::fs::create_dir_all(target_dir).map_err(Error::Io)?;
        let root = target_dir.canonicalize().map_err(Error::Io)?;
        let aborted = || {
            abort_signal
                .as_ref()
                .is_some_and(|s| s.load(Ordering::Relaxed))
        };

        // Folders and links first, in order.
        let mut regular = Vec::new();
        for file in manifest.files() {
            if aborted() {
                return Ok(());
            }
            let linktarget = file.linktarget();
            let entry = if file.flags() & manifest::file::FLAG_DIRECTORY != 0 {
                paths::Entry::Dir
            } else if !linktarget.is_empty() || file.flags() & manifest::file::FLAG_SYMLINK != 0 {
                // Symlink entries without a linktarget field carry the
                // target path as their chunk content instead.
                let raw_target = if !linktarget.is_empty() {
                    linktarget
                } else {
                    if file.size() > paths::MAX_LINK_TARGET {
                        return Err(Error::UnsafePath(format!(
                            "symlink entry {} claims a {}-byte target",
                            file.full_path(),
                            file.size()
                        )));
                    }
                    let bytes = file.download_to_memory(key_arr).await?;
                    String::from_utf8(bytes).map_err(|e| Error::Unexpected(e.to_string()))?
                };
                paths::Entry::Link(
                    raw_target
                        .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                        .replace('\\', "/"),
                )
            } else {
                paths::Entry::File
            };
            if let Some(full_path) = paths::place(&root, &file.full_path(), entry)? {
                regular.push((file, full_path));
            }
        }

        // Files in parallel, one chunk budget.
        let limit = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
        futures::stream::iter(regular.into_iter().map(Ok::<_, Error>))
            .try_for_each_concurrent(CONCURRENCY, |(file, full_path)| {
                let limit = limit.clone();
                let abort_signal = abort_signal.clone();
                let on_progress = on_progress.clone();
                async move {
                    if abort_signal
                        .as_ref()
                        .is_some_and(|s| s.load(Ordering::Relaxed))
                    {
                        return Ok(());
                    }
                    if file.size() > 0 {
                        file.download(
                            key_arr,
                            &full_path,
                            verify_mode,
                            abort_signal,
                            Some(limit),
                            on_progress,
                        )
                        .await
                    } else {
                        if !full_path.is_file() {
                            std::fs::File::create(&full_path).map_err(Error::Io)?;
                        }
                        file.apply_unix_mode(&full_path).await?;
                        Ok(())
                    }
                }
            })
            .await?;
        Ok(())
    }

    pub async fn get_manifest(
        &self,
        app_id: u32,
        depot_id: u32,
        manifest_id: u64,
        request_code: Option<u64>,
        depot_key: Option<[u8; 32]>,
    ) -> Result<DepotManifest, Error> {
        let bytes = self
            .inner
            .get_manifest_bytes(depot_id, manifest_id, request_code, MANIFEST_VERSION)
            .await?;

        let mut manifest = DepotManifest::deserialize(
            self.inner.clone(),
            app_id,
            depot_id,
            manifest_id,
            &bytes[..],
        )?;
        if manifest.filenames_encrypted()
            && let Some(key) = depot_key
        {
            manifest.decrypt_filenames(key)?;
        }

        Ok(manifest)
    }
}
