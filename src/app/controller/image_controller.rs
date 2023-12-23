use crate::app::{
    api_controller,
    app_state::model::{ImageDataRaw, ImageDataRef},
    asset_controller::AssetRef,
};

use image::ImageFormat;
use tokio::sync::oneshot;

use std::sync::Arc;

use super::AssetCommand;
use anyhow::anyhow;

pub struct ImageController {}

impl ImageController {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn load_game_avatar_if_empty(
        &self,
        parent: Arc<super::ControllerHub>,
        info: &api_controller::GameInfo,
        image_ref: ImageDataRef,
    ) {
        match image_ref.read().await.loaded_image {
            ImageDataRaw::Empty => {}
            _ => return,
        };

        if !info.info.status.avatar.id.is_empty() {
            let mut path = arkhost_api::consts::asset::api::avatar(
                &info.info.status.avatar.type_val,
                &info.info.status.avatar.id,
            );
            path.push_str(".webp");
            {
                let mut image_ref = image_ref.write().await;
                image_ref.asset_path = path.clone();
                image_ref.cache_key = Some(path);
                image_ref.format = Some(ImageFormat::WebP);
            }
            self.load_image(parent.clone(), image_ref).await;
        }
    }

    pub async fn load_game_char_illust_if_empty(
        &self,
        parent: Arc<super::ControllerHub>,
        info: &api_controller::GameInfo,
        image_ref: ImageDataRef,
    ) {
        match image_ref.read().await.loaded_image {
            ImageDataRaw::Empty => {}
            _ => return,
        };

        if let Some(details) = &info.details {
            if !details.status.secretary_skin_id.is_empty() {
                let mut skin_id = details.status.get_secretary_skin_id_escaped();
                skin_id.push_str(".webp");
                let path = arkhost_api::consts::asset::api::charpack(&skin_id);
                {
                    let mut image_ref = image_ref.write().await;
                    image_ref.asset_path = path.clone();
                    image_ref.cache_key = Some(path);
                    image_ref.format = Some(ImageFormat::WebP);
                }
                let parent = parent.clone();
                self.load_image(parent.clone(), image_ref).await;
            }
        }
    }

    pub async fn load_image(&self, parent: Arc<super::ControllerHub>, image_ref: ImageDataRef) {
        let (path, cache_key, src_format) = {
            let mut image_ref = image_ref.write().await;
            image_ref.loaded_image = ImageDataRaw::Pending;
            (
                image_ref.asset_path.clone(),
                image_ref.cache_key.clone(),
                image_ref.format.clone(),
            )
        };
        let (resp, mut rx) = oneshot::channel();
        match parent
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
                eprintln!("Error loading image: {:?} {:?}", image_ref.read().await, e);
            }
        }
    }
}
