use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use url::Url;

use crate::app::game_data;

#[derive(Debug, Clone)]
pub enum AssetPath {
    GameAsset(String),
    External(Url),
}

impl AssetPath {
    pub fn inner_path(&self) -> &str {
        match self {
            Self::GameAsset(path) => path,
            Self::External(url) => url.as_str(),
        }
    }
}

impl Default for AssetPath {
    fn default() -> Self {
        Self::GameAsset(String::new())
    }
}

#[derive(Default, Debug, Clone)]
pub enum ImageDataRaw {
    #[default]
    Empty,
    Rgba8 {
        raw: bytes::Bytes,
        width: u32,
        height: u32,
    },
}

#[derive(Default, Debug, Clone)]
pub struct ImageData {
    pub asset_path: AssetPath,
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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub data_saver_mode_enabled: bool,
    #[serde(default)]
    pub last_ssr_record_ts: DateTime<Utc>,
    #[serde(default)]
    /// 缓存的作战截图URL
    pub cached_battle_screenshots: HashMap<String, Vec<url::Url>>,
}
