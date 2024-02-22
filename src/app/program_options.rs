use argh::FromArgs;

#[derive(Debug, Clone, FromArgs)]
/// 程序参数
pub struct LaunchArgs {
    #[argh(switch)]
    /// 是否启动 AppWindow
    pub launch_app_window: Option<bool>,

    #[argh(switch)]
    /// 是否启动 WebView
    pub launch_webview: Option<bool>,

    #[argh(option)]
    /// 要验证的账号
    pub account: Option<String>,

    #[argh(option)]
    /// 父进程的 IPC Server 名称
    pub ipc: Option<String>,
}
