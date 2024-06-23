use crate::app::{
    api_user_model,
    app_state::model::{AssetPath, ImageData, ImageDataRaw, ImageDataRef},
    asset_worker::AssetRef,
};

use arkhost_api::models::api_arkhost::Avatar;
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
        game: &api_user_model::GameEntry,
        image_ref: ImageDataRef,
    ) {
        match image_ref.read().await.loaded_image {
            ImageDataRaw::Empty => {}
            _ => return,
        };

        if !game.info.status.avatar.id.is_empty() {
            self.load_avatar(&game.info.status.avatar, image_ref).await;
        }
    }

    pub async fn load_game_char_illust_if_empty(
        &self,
        game: &api_user_model::GameEntry,
        image_ref: ImageDataRef,
    ) {
        match image_ref.read().await.loaded_image {
            ImageDataRaw::Empty => {}
            _ => return,
        };

        if let Some(details) = &game.details {
            if !details.status.secretary_skin_id.is_empty() {
                let mut skin_id = details.status.sanitize_secretary_skin_id_for_url();
                skin_id.push_str(".png");
                let path: String = arkhost_api::consts::asset::assets::charpack(&skin_id);
                self.load_image_to_ref(
                    AssetPath::GameAsset(path),
                    None,
                    Some(ImageFormat::Png),
                    image_ref,
                )
                .await;
            }
        }
    }

    pub async fn load_avatar(&self, avatar: &Avatar, image_ref: ImageDataRef) {
        let mut path = arkhost_api::consts::asset::assets::avatar(
            &avatar.type_val,
            &avatar.sanitize_id_for_url(),
        );
        path.push_str(".png");
        self.load_image_to_ref(
            AssetPath::GameAsset(path),
            None,
            Some(ImageFormat::Png),
            image_ref,
        )
        .await;
    }

    pub async fn load_image_to_ref(
        &self,
        path: AssetPath,
        cache_key: Option<String>,
        image_format: Option<ImageFormat>,
        image_ref: ImageDataRef,
    ) {
        let mut image_ref: tokio::sync::RwLockWriteGuard<ImageData> = image_ref.write().await;
        image_ref.asset_path = path;
        image_ref.cache_key = cache_key;
        image_ref.format = image_format;

        self.load_image_to_data(&mut image_ref).await;
    }

    pub async fn load_image_to_data(&self, image_data: &mut ImageData) {
        let (path, cache_key, src_format) = (
            image_data.asset_path.clone(),
            image_data.cache_key.clone(),
            image_data.format,
        );
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
                image_data.loaded_image = loaded_image;
            }
            Err(e) => {
                image_data.loaded_image = ImageDataRaw::Empty;
                if !self
                    .errored_resource_urls
                    .read()
                    .await
                    .contains(image_data.asset_path.inner_path())
                {
                    self.errored_resource_urls
                        .write()
                        .await
                        .insert(image_data.asset_path.inner_path().to_owned());
                    println!("[Controller] Error loading image (further errors from this URL will be suppressed): {:?} {:?}", image_data, e);
                }
            }
        }
    }
}
