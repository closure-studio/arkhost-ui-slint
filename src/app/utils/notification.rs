use std::time::Duration;

use notify_rust::Notification;

pub fn toast(summary: &str, subtitle: Option<&str>, body: &str, duration: Option<Duration>) {
    let mut notification = Notification::new();
    notification
        .appname(consts::APP_NAME)
        .summary(summary)
        .body(body)
        .timeout(duration.unwrap_or(Duration::from_millis(consts::DEFAULT_TIMEOUT_MS)));
    // TODO: 安装快捷方式到开始屏幕并自定义AppUserModelID
    // #[cfg(target_os = "windows")]
    // notification.app_id(app_user_model::consts::DEFAULT_ID);
    if let Some(subtitle) = subtitle {
        notification.subtitle(subtitle);
    }

    let res = notification.show();
    log_on_show_failed(res);
}

fn log_on_show_failed<T>(result: Result<T, notify_rust::error::Error>) {
    if let Err(e) = result {
        println!("[Notification] Error showing notification: {e}");
    }
}

pub mod consts {
    pub const APP_NAME: &str = "Closure Studio";
    pub const DEFAULT_TIMEOUT_MS: u64 = 4000;
}
