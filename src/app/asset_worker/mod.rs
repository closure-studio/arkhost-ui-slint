use std::{collections::HashMap, sync::Arc};

use anyhow::bail;
use arkhost_api::clients::asset::AssetClient;
use bytes::Buf;
use derivative::Derivative;
use reqwest::Response;
use semver::Version;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_util::sync::CancellationToken;

use arkhost_ota::{Release, ReleaseIndexV1};

use super::utils::app_metadata;

pub type CommandResult<T> = anyhow::Result<T>;
pub type Responder<T> = oneshot::Sender<CommandResult<T>>;

#[derive(Debug, Clone)]
pub enum AssetRef {
    Bytes(bytes::Bytes),
    Rgba8Image {
        raw: bytes::Bytes,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ReleaseUpdateType {
    Patch,
    FullDownload,
}

#[derive(Derivative)]
#[derivative(Debug)]
#[allow(unused)]
pub enum Command {
    LoadAsset {
        cache_key: Option<String>,
        path: String,
        resp: Responder<AssetRef>,
    },
    LoadImageRgba8 {
        cache_key: Option<String>,
        path: String,
        src_format: Option<image::ImageFormat>,
        resp: Responder<AssetRef>,
    },
    RetrieveCache {
        cache_key: String,
        resp: oneshot::Sender<Option<AssetRef>>,
    },
    DeleteCache {
        cache_key: String,
    },
    CheckReleaseUpdate {
        branch: Option<String>,
        mode: ReleaseUpdateType,
        resp: Responder<Option<(Release, usize, String)>>,
    },
    DownloadReleaseUpdate {
        branch: Option<String>,
        mode: ReleaseUpdateType,
        resp: Responder<(Release, usize, String, Response)>,
    },
}

pub struct AssetWorker {
    pub asset_client: Arc<AssetClient>,
    pub cache: Arc<RwLock<HashMap<String, AssetRef>>>,
}

impl AssetWorker {
    pub fn new(asset_client: AssetClient) -> Self {
        Self {
            asset_client: Arc::new(asset_client),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn run(&mut self, mut recv: mpsc::Receiver<Command>, stop: CancellationToken) {
        tokio::select! {
            _ = async {
                while let Some(cmd) = recv.recv().await {
                    match cmd {
                        Command::LoadAsset {
                            cache_key,
                            path,
                            resp,
                        } => _ = resp.send(self.load_asset(cache_key, &path).await),
                        Command::LoadImageRgba8 {
                            cache_key,
                            path,
                            src_format,
                            resp,
                        } => _ = resp.send(self.load_image_rgba8(cache_key, &path, src_format).await),
                        Command::RetrieveCache { cache_key, resp } => {
                            _ = resp.send(self.read_cache_by_key(&Some(cache_key)).await)
                        }
                        Command::DeleteCache { cache_key } => {
                            self.delete_cache_by_key(&Some(cache_key)).await
                        }
                        Command::CheckReleaseUpdate { branch, mode, resp } => {
                            _ = resp.send(self.check_release_update(branch.as_ref().map_or(arkhost_ota::consts::DEFAULT_BRANCH, |x| x), mode).await)
                        },
                        Command::DownloadReleaseUpdate { branch, mode, resp } => {
                            _ = resp.send(self.download_release_update(branch.as_ref().map_or(arkhost_ota::consts::DEFAULT_BRANCH, |x| x), mode).await)
                        },
                    }
                }
            } => {},
            _ = stop.cancelled() => {}
        }
    }

    pub async fn load_asset(
        &self,
        cache_key: Option<String>,
        path: &str,
    ) -> CommandResult<AssetRef> {
        if let Some(asset) = self.read_cache_by_key(&cache_key).await {
            return Ok(asset);
        }

        let bytes = self.asset_client.get_content(path, |x| x).await?;
        let asset = AssetRef::Bytes(bytes);
        self.write_cache_by_key(cache_key, &asset).await;
        Ok(asset)
    }

    pub async fn load_image_rgba8(
        &self,
        cache_key: Option<String>,
        path: &str,
        src_format: Option<image::ImageFormat>,
    ) -> CommandResult<AssetRef> {
        if let Some(bytes) = self.read_cache_by_key(&cache_key).await {
            return Ok(bytes);
        }

        let src_bytes = self.asset_client.get_content(path, |x| x).await?;
        let image = match src_format {
            Some(fmt) => image::load_from_memory_with_format(&src_bytes, fmt)?,
            None => image::load_from_memory(&src_bytes)?,
        };
        let (width, height) = (image.width(), image.height());
        let bytes = bytes::Bytes::from(image.into_rgba8().into_raw());
        let asset = AssetRef::Rgba8Image {
            raw: bytes,
            width,
            height,
        };
        self.write_cache_by_key(cache_key, &asset).await;
        Ok(asset)
    }

    pub async fn read_cache_by_key(&self, cache_key: &Option<String>) -> Option<AssetRef> {
        if let Some(key) = cache_key {
            return self.cache.read().await.get(key).cloned();
        }
        None
    }

    pub async fn write_cache_by_key(&self, cache_key: Option<String>, asset: &AssetRef) {
        if let Some(key) = cache_key {
            self.cache.write().await.insert(key, asset.clone());
        }
    }

    pub async fn delete_cache_by_key(&self, cache_key: &Option<String>) {
        if let Some(key) = cache_key {
            self.cache.write().await.remove(key);
        }
    }

    pub async fn check_release_update(
        &self,
        branch: &str,
        mode: ReleaseUpdateType,
    ) -> CommandResult<Option<(Release, usize, String)>> {
        let force_update = crate::app::env::force_update();
        if !force_update && cfg!(debug_assertions) {
            return Ok(None);
        }
        let cur_version = match app_metadata::CARGO_PKG_VERSION {
            Some(version) => Version::parse(version)?,
            None => bail!("no version found in APP metadata"),
        };
        let release_pub_key = match arkhost_ota::release_public_key() {
            Some(key) => key,
            None => bail!("no valid release public key found"),
        };
        let index_bytes = self
            .asset_client
            .get_content(arkhost_ota::consts::asset::ui::ota::v1::INDEX, |x| x)
            .await?;
        let sig_bytes = self
            .asset_client
            .get_content(arkhost_ota::consts::asset::ui::ota::v1::INDEX_SIG, |x| x)
            .await?;
        let sig = arkhost_ota::try_parse_detached_signature(&sig_bytes)?;
        sig.verify(release_pub_key, index_bytes.clone().reader())?;

        let index: ReleaseIndexV1 = serde_json::de::from_slice(&index_bytes)?;
        let release = match index.branches.get(branch) {
            Some(release) => release,
            None => bail!("unable to find release of branch '{}'", branch),
        };
        let self_hash = app_metadata::executable_sha256()?;
        let release_hash = hex::decode(&release.file.hash)?;
        if !force_update && (self_hash[..] == release_hash[..] || release.version <= cur_version) {
            return Ok(None);
        }
        let mut path = arkhost_ota::consts::asset::ui::ota::v1::FILES.to_owned();
        match mode {
            ReleaseUpdateType::Patch => {
                path.push_str(&arkhost_ota::file_bspatch_path(
                    &release.file,
                    &hex::encode(*self_hash),
                ));
            }
            ReleaseUpdateType::FullDownload => {
                path.push_str(&arkhost_ota::file_path(&release.file));
            }
        };
        let resp = self
            .asset_client
            .head_content(&path, |x| x)
            .await?
            .error_for_status()?;
        Ok(Some((
            release.clone(),
            resp.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|x| x.to_str().ok())
                .and_then(|x| x.parse().ok())
                .unwrap_or(0usize),
            path,
        )))
    }

    pub async fn download_release_update(
        &self,
        branch: &str,
        mode: ReleaseUpdateType,
    ) -> CommandResult<(Release, usize, String, Response)> {
        let (release, download_size, path) = match self.check_release_update(branch, mode).await? {
            Some(x) => x,
            None => bail!(
                "unable to update with params: branch: '{}', mode: {:?}",
                branch,
                mode
            ),
        };

        let response = self.asset_client.get_content_response(&path, |x| x).await?;
        Ok((release, download_size, path, response))
    }
}
