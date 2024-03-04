#[cfg(target_os = "windows")]
pub mod app_user_model;
pub mod app_metadata;
pub mod cache_manager;
pub mod data_dir;
pub mod ext_link;
pub mod levenshtein_distance;
pub mod notification;
#[cfg(feature = "desktop-app")]
pub mod subprocess;
pub mod user_state;
#[cfg(target_os = "windows")]
/// 检测 Microsoft Edge WebView2 是否安装，Windows下可用。
pub mod webview2;
