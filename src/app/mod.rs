/// API请求处理器，用于接收API命令
pub mod api_worker;
/// AppState，管理UI中状态及其数据映射（Mapping）
pub mod app_state;
/// 资源处理器，用于接收资源命令并请求资源文件及缓存等
pub mod asset_worker;
/// 验证处理器，用于接收处理验证命令并在WebView窗口中进行用户验证/游戏验证
pub mod auth_worker;
/// UI控制器类，用于在Rust运行时和UI组件之间传输数据和执行操作
pub mod controller;
/// 游戏资源数据类，用于关卡信息显示、立绘定位等
pub mod game_data;
/// 运行时API数据模型
pub mod rt_api_model;
/// 工具类
pub mod utils;

/// 环境（变量）相关
pub mod env;
#[cfg(feature = "desktop-app")]
/// 用于在桌面端中处理UI进程和验证网页弹窗进程的通讯
pub mod ipc_auth_comm;
/// APP OTA 功能相关类型
pub mod ota;
/// 启动参数
pub mod program_options;
/// Slint codegen
#[allow(clippy::all)]
pub mod ui;
/// 用于显示网页弹窗或WebView来进行用户验证/游戏验证
pub mod webview;
