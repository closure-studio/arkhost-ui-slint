/// API请求控制器
pub mod api_controller;
/// 运行时API数据模型
pub mod rt_api_model;
/// 验证控制器，用于在WebView窗口中进行用户验证/游戏验证
pub mod auth_controller;
/// UI控制器类，用于在Rust运行时和UI组件之间传输数据和执行操作
pub mod controller;
/// 资源控制器，用于请求资源文件及缓存等
pub mod asset_controller;
/// 游戏资源数据类，用于关卡信息显示、立绘定位等
pub mod game_data;
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