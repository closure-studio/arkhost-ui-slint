pub mod ext_link;
#[cfg(feature = "desktop-app")]
pub mod subprocess;
pub mod data_dir;
pub mod user_state;
pub mod cache_manager;
#[cfg(target_os = "windows")]
/// 检测 Microsoft Edge WebView2 是否安装，Windows下可用。
pub mod webview2;
pub mod notification;
#[cfg(target_os = "windows")]
pub mod app_user_model;