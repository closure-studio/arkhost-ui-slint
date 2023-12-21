/// API请求控制器
pub mod api_controller;
/// 验证控制器，用于在WebView窗口中进行用户验证/游戏验证
pub mod auth_controller;
/// Controller，用于在Rust运行时和UI组件之间传输数据和执行操作
pub mod controller;
/// AppState，UI状态
pub mod app_state;
/// 工具类
pub mod utils;

#[cfg(feature = "desktop-app")]
/// 用于在桌面端中处理UI进程和验证网页弹窗进程的通讯
pub mod ipc;
/// 用于显示网页弹窗或WebView来进行用户验证/游戏验证
pub mod webview;
/// Slint生成的代码导出至此
pub mod ui;