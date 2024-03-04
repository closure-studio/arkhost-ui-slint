use std::sync::Arc;
use tokio::sync::RwLock;

use crate::app::game_data;

#[derive(Default, Debug, Clone)]
pub enum ImageDataRaw {
    #[default]
    Empty,
    Pending,
    Rgba8 {
        raw: bytes::Bytes,
        width: u32,
        height: u32,
    },
}

#[derive(Default, Debug, Clone)]
pub struct ImageData {
    pub asset_path: String,
    pub cache_key: Option<String>,
    pub format: Option<image::ImageFormat>,
    pub loaded_image: ImageDataRaw,
}

impl ImageData {
    pub fn to_slint_image(&self) -> Option<slint::Image> {
        match &self.loaded_image {
            ImageDataRaw::Rgba8 { raw, width, height } => Some(slint::Image::from_rgba8(
                slint::SharedPixelBuffer::clone_from_slice(raw, *width, *height),
            )),
            _ => None,
        }
    }
}

pub type ImageDataRef = Arc<RwLock<ImageData>>;

#[derive(Default, Debug, Clone)]
pub struct CharIllust {
    pub image: ImageData,
    pub positions: game_data::CharPack,
}
