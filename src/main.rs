// TODO: 添加用户界面报错后隐藏命令行输出
// #![windows_subsystem = "windows"]

mod app;

use app::app_state::AppState;
#[cfg(feature = "desktop-app")]
use app::auth_controller::ipc::IpcAuthController;

use app::auth_controller::AuthController;
use app::ui::*;
use app::utils::user_state::{UserStateFileStorage, UserStateFileStoreSetting};
use app::{api_controller::Controller as ApiController, controller::Controller};
use arkhost_api::clients::common::UserState;
use std::sync::{Arc, RwLock};
use tokio::{self, sync::mpsc};

async fn run_app() -> Result<(), slint::PlatformError> {
    #[cfg(feature = "desktop-app")]
    let mut user_state =
        UserStateFileStorage::new(UserStateFileStoreSetting::HomeDirWithCurrentDirFallback);

    user_state.load_from_file();
    let user_state_loaded = user_state.get_login_state().is_some();

    let mut api_controller = ApiController::new(Arc::new(RwLock::new(user_state)));
    let (tx_api_command, rx_api_command) = mpsc::channel(128);
    tokio::task::spawn(async move {
        api_controller.run(rx_api_command).await;
    });

    #[cfg(feature = "desktop-app")]
    let mut auth_controller = IpcAuthController::new();

    let (tx_auth_command, rx_auth_command) = mpsc::channel(16);
    tokio::task::spawn(async move {
        auth_controller.run(rx_auth_command).await;
    });

    let ui = AppWindow::new()?;
    let controller = Arc::new(Controller::new(AppState::new(ui.as_weak())));
    controller
        .clone()
        .attach(&ui, tx_api_command.clone(), tx_auth_command.clone());
    if user_state_loaded {
        tokio::task::spawn(controller.clone().auth(tx_api_command.clone()));
    }

    let result = ui.run();

    let _ = tx_api_command
        .send(app::api_controller::Command::Stop {})
        .await;
    let _ = tx_auth_command
        .send(app::auth_controller::Command::Stop {})
        .await;

    result
}

#[tokio::main()]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "desktop-app")]
    if let Some(result) = app::webview::auth::subprocess_webview::launch_if_requested() {
        result?;
        return Ok(());
    }

    let app_window_result = run_app().await;

    app_window_result?;
    Ok(())
}
