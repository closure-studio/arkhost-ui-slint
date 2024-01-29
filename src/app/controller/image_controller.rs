use crate::app::{
    app_state::model::{ImageDataRaw, ImageDataRef},
    asset_controller::AssetRef,
    rt_api_model,
};

use image::ImageFormat;
use tokio::sync::{oneshot, RwLock};

use std::{collections::HashSet, sync::Arc};

use super::{sender::Sender, AssetCommand};
use anyhow::anyhow;

pub struct ImageController {
    sender: Arc<Sender>,
    errored_resource_urls: RwLock<HashSet<String>>,
}

impl ImageController {
    pub fn new(sender: Arc<Sender>) -> Self {
        Self {
            sender,
            errored_resource_urls: RwLock::new(HashSet::new()),
        }
    }

    pub async fn load_game_avatar_if_empty(
        &self,
        game: &rt_api_model::GameEntry,
        image_ref: ImageDataRef,
    ) {
        match image_ref.read().await.loaded_image {
            ImageDataRaw::Empty => {}
            _ => return,
        };

        if !game.info.status.avatar.id.is_empty() {
            let mut path = arkhost_api::consts::asset::api::avatar(
                &game.info.status.avatar.type_val,
                &game.info.status.avatar.sanitize_id_for_url(),
            );
            path.push_str(".webp");
            {
                let mut image_ref = image_ref.write().await;
                image_ref.asset_path = path.clone();
                image_ref.cache_key = Some(path);
                image_ref.format = Some(ImageFormat::WebP);
            }
            self.load_image(image_ref).await;
        }
    }

    pub async fn load_game_char_illust_if_empty(
        &self,
        game: &rt_api_model::GameEntry,
        image_ref: ImageDataRef,
    ) {
        match image_ref.read().await.loaded_image {
            ImageDataRaw::Empty => {}
            _ => return,
        };

        if let Some(details) = &game.details {
            if !details.status.secretary_skin_id.is_empty() {
                let mut skin_id = details.status.sanitize_secretary_skin_id_for_url();
                skin_id.push_str(".webp");
                let path: String = arkhost_api::consts::asset::api::charpack(&skin_id);
                {
                    let mut image_ref = image_ref.write().await;
                    image_ref.asset_path = path.clone();
                    image_ref.cache_key = Some(path);
                    image_ref.format = Some(ImageFormat::WebP);
                }
                self.load_image(image_ref).await;
            }
        }
    }

    pub async fn load_image(&self, image_ref: ImageDataRef) {
        let (path, cache_key, src_format) = {
            let mut image_ref = image_ref.write().await;
            image_ref.loaded_image = ImageDataRaw::Pending;
            (
                image_ref.asset_path.clone(),
                image_ref.cache_key.clone(),
                image_ref.format,
            )
        };
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_asset_request(
                AssetCommand::LoadImageRgba8 {
                    path,
                    cache_key,
                    src_format,
                    resp,
                },
                &mut rx,
            )
            .await
            .and_then(|asset| match asset {
                AssetRef::Rgba8Image { raw, width, height } => {
                    Ok(ImageDataRaw::Rgba8 { raw, width, height })
                }
                _ => Err(anyhow!("unexpected AssetRef: {asset:?}")),
            }) {
            Ok(loaded_image) => {
                image_ref.write().await.loaded_image = loaded_image;
            }
            Err(e) => {
                image_ref.write().await.loaded_image = ImageDataRaw::Empty;
                if !self
                    .errored_resource_urls
                    .read()
                    .await
                    .contains(&image_ref.read().await.asset_path)
                {
                    self.errored_resource_urls
                        .write()
                        .await
                        .insert(image_ref.read().await.asset_path.clone());
                    eprintln!("[Controller] Error loading image (further errors from this URL will be suppressed): {:?} {:?}", image_ref.read().await, e);
                }
            }
        }
    }
}
