use argh::FromArgs;

#[derive(FromArgs)]
/// 程序参数
pub struct LaunchArgs {
    #[argh(switch)]
    /// 附加到命令行窗口，等同于设置环境变量
    /// ARKHOST_APP_ATTACH_CONSOLE='1'
    pub attach_console: Option<bool>,

    #[argh(switch)]
    /// 在OTA存在任意版本时将其作为可更新选项，等同于设置环境变量
    /// ARKHOST_APP_FORCE_UPDATE='1'
    pub force_update: Option<bool>,

    #[argh(option)]
    /// 指定资源服务器，等同于设置环境变量
    /// ARKHOST_APP_OVERRIDE_ASSET_SERVER=<asset_server>
    pub asset_server: Option<String>,

    #[argh(subcommand)]
    pub launch_spec: Option<LaunchSpec>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
pub enum LaunchSpec {
    AppWindow(LaunchAppWindowArgs),
    WebView(LaunchWebViewArgs),
}

#[derive(FromArgs)]
#[argh(subcommand, name = "app")]
/// 启动 AppWindow
pub struct LaunchAppWindowArgs {}

#[derive(FromArgs)]
#[argh(subcommand, name = "webview")]

/// 启动 WebView
pub struct LaunchWebViewArgs {
    #[argh(option)]
    /// 用户账号（WebView）
    pub account: String,

    #[argh(option)]
    /// 父进程 IPC Server 名称（WebView）
    pub ipc: String,
}
