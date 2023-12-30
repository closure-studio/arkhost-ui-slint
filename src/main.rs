// TODO: 添加用户界面报错后隐藏命令行输出
// #![windows_subsystem = "windows"]

mod app;

use app::app_state::AppState;
use app::asset_controller::AssetController;
#[cfg(feature = "desktop-app")]
use app::auth_controller::ipc::IpcAuthController;

use app::auth_controller::AuthController;
use app::ui::*;
use app::utils::data_dir::get_data_dir;
use app::utils::user_state::{UserStateFileStorage, UserStateFileStoreSetting};
use app::{api_controller::Controller as ApiController, controller::ControllerHub};
use arkhost_api::clients::asset::AssetClient;
use arkhost_api::clients::common::UserState;
use arkhost_api::clients::{common::UserStateDataSource, id_server::AuthClient};
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::sync::{Arc, RwLock};
use tokio::{self, sync::mpsc};

fn create_auth_client(user_state: Arc<RwLock<dyn UserState>>) -> AuthClient {
    let client = AuthClient::get_client_builder_with_default_settings()
        .build()
        .unwrap();

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client_with_middlewares = reqwest_middleware::ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    AuthClient::new(
        arkhost_api::consts::passport::API_BASE_URL,
        client_with_middlewares,
        user_state,
    )
}

fn create_asset_client() -> AssetClient {
    let client = AssetClient::get_client_builder_with_default_settings()
        .build()
        .unwrap();

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client_with_middlewares = reqwest_middleware::ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager {
                path: get_data_dir().join("cache/assets/"),
            },
            options: HttpCacheOptions {
                cache_mode_fn: Some(Arc::new(|req| {
                    // TODO: temporary fix
                    if req.uri.path().ends_with(".json") || req.uri.path().contains("/avatar/") {
                        CacheMode::NoStore
                    } else {
                        CacheMode::Default
                    }
                })),
                ..Default::default()
            },
        }))
        .build();

    AssetClient::new(
        arkhost_api::consts::asset::API_BASE_URL,
        client_with_middlewares,
    )
}

// TODO: 改进初始化APP相关代码质量
async fn run_app() -> Result<(), slint::PlatformError> {
    #[cfg(feature = "desktop-app")]
    let mut user_state =
        UserStateFileStorage::new(UserStateFileStoreSetting::DataDirWithCurrentDirFallback);

    user_state.load_from_file();
    let user_state_data_or_null = user_state.get_user_state_data();
    let auth_client = create_auth_client(Arc::new(RwLock::new(user_state)));

    let mut api_controller = ApiController::new(auth_client);
    let (tx_api_command, rx_api_command) = mpsc::channel(32);
    tokio::task::spawn(async move {
        api_controller.run(rx_api_command).await;
    });

    #[cfg(feature = "desktop-app")]
    let mut auth_controller = IpcAuthController::new();
    let (tx_auth_command, rx_auth_command) = mpsc::channel(16);
    tokio::task::spawn(async move {
        auth_controller.run(rx_auth_command).await;
    });

    let asset_client = create_asset_client();
    let mut asset_controller = AssetController::new(asset_client);
    let (tx_asset_command, rx_asset_command) = mpsc::channel(32);
    tokio::task::spawn(async move {
        asset_controller.run(rx_asset_command).await;
    });

    let ui = AppWindow::new()?;
    let hub = Arc::new(ControllerHub::new(
        AppState::new(ui.as_weak()),
        tx_api_command.clone(),
        tx_auth_command.clone(),
        tx_asset_command.clone(),
    ));
    hub.clone().attach(&ui);
    if let Some(state) = user_state_data_or_null {
        let app_state = hub.app_state.lock().unwrap();
        if state.is_expired() {
            app_state.set_login_state(LoginState::Unlogged, "登录已过期，请重新登录".into());
            app_state.set_use_auth(String::new(), false);
        } else {
            app_state.set_use_auth(state.account, true);
            let controller = hub.clone();
            tokio::task::spawn(async move {
                controller.account_controller.auth(controller.clone()).await
            });
        }
    }

    let result = ui.run();

    let _ = tx_api_command
        .send(app::api_controller::Command::Stop {})
        .await;
    let _ = tx_auth_command
        .send(app::auth_controller::Command::Stop {})
        .await;
    let _ = tx_asset_command
        .send(app::asset_controller::Command::Stop {})
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
    println!("[main] APP exited on close requested.");
    Ok(())
}
