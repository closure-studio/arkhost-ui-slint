use std::{collections::HashMap, sync::Arc};

use anyhow::Ok;
use arkhost_api::clients::asset::AssetClient;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_util::sync::CancellationToken;

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

#[derive(Debug)]
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
                        } => _ = resp.send(self.load_asset(cache_key, path).await),
                        Command::LoadImageRgba8 {
                            cache_key,
                            path,
                            src_format,
                            resp,
                        } => _ = resp.send(self.load_image_rgba8(cache_key, path, src_format).await),
                        Command::RetrieveCache { cache_key, resp } => {
                            _ = resp.send(self.read_cache_by_key(&Some(cache_key)).await)
                        }
                        Command::DeleteCache { cache_key } => {
                            self.delete_cache_by_key(&Some(cache_key)).await
                        }
                    }
                }
            } => {},
            _ = stop.cancelled() => {}
        }
    }

    pub async fn load_asset(
        &self,
        cache_key: Option<String>,
        path: String,
    ) -> CommandResult<AssetRef> {
        if let Some(asset) = self.read_cache_by_key(&cache_key).await {
            return Ok(asset);
        }

        let bytes = self.asset_client.get_content(path).await?;
        let asset = AssetRef::Bytes(bytes);
        self.write_cache_by_key(cache_key, &asset).await;
        Ok(asset)
    }

    pub async fn load_image_rgba8(
        &self,
        cache_key: Option<String>,
        path: String,
        src_format: Option<image::ImageFormat>,
    ) -> CommandResult<AssetRef> {
        if let Some(bytes) = self.read_cache_by_key(&cache_key).await {
            return Ok(bytes);
        }

        let src_bytes = self.asset_client.get_content(path).await?;
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
}
