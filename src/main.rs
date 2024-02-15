// TODO: 添加用户界面报错后隐藏命令行输出
// #![windows_subsystem = "windows"]

mod app;

use app::app_state::AppState;
use app::asset_worker::AssetWorker;
#[cfg(feature = "desktop-app")]
use app::auth_worker::ipc::IpcAuthWorker;

use app::auth_worker::AuthWorker;
use app::controller::rt_api_model::RtApiModel;
use app::ui::*;
use app::utils::cache_manager::CACacheManager;
use app::utils::data_dir::data_dir;
use app::utils::user_state::{UserStateFileStorage, UserStateFileStoreSetting};
use app::{api_worker::Worker as ApiWorker, controller::ControllerAdaptor};
use arkhost_api::clients::asset::AssetClient;
use arkhost_api::clients::common::UserState;
use arkhost_api::clients::{common::UserStateDataSource, id_server::AuthClient};
use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::{self, sync::mpsc};
use tokio_util::sync::CancellationToken;

fn create_auth_client(user_state: Arc<RwLock<dyn UserState>>) -> AuthClient {
    let client = AuthClient::default_client_builder()
        .use_rustls_tls()
        .gzip(true)
        .brotli(true)
        .timeout(Duration::from_secs(12))
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
    let client = AssetClient::default_client_builder()
        .use_rustls_tls()
        .gzip(true)
        .brotli(true)
        .timeout(Duration::from_secs(12))
        .build()
        .unwrap();

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client_with_middlewares = reqwest_middleware::ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager {
                path: data_dir().join("cache/assets/"),
            },
            options: HttpCacheOptions {
                cache_mode_fn: Some(Arc::new(|req| {
                    if req.uri.path().ends_with(".webp") {
                        CacheMode::ForceCache
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
    let user_state_data_or_null = user_state.user_state_data();
    let auth_client = create_auth_client(Arc::new(RwLock::new(user_state)));

    let stop = CancellationToken::new();
    let _guard = stop.clone().drop_guard();

    let mut api_worker = ApiWorker::new(auth_client);
    let (tx_api_command, rx_api_command) = mpsc::channel(32);
    tokio::spawn({
        let stop = stop.clone();
        async move {
            api_worker.run(rx_api_command, stop).await;
        }
    });
    

    #[cfg(feature = "desktop-app")]
    let mut auth_worker = IpcAuthWorker::new();
    let (tx_auth_command, rx_auth_command) = mpsc::channel(16);
    let stop = stop.clone();
    tokio::spawn({
        let stop = stop.clone();
        async move {
            auth_worker.run(rx_auth_command, stop).await;
        }
    });

    let asset_client = create_asset_client();
    let mut asset_worker = AssetWorker::new(asset_client);
    let (tx_asset_command, rx_asset_command) = mpsc::channel(32);
    tokio::spawn({
        let stop = stop.clone();
        async move {
            asset_worker.run(rx_asset_command, stop).await;
        }
    });

    let ui = AppWindow::new()?;
    let adaptor = Arc::new(ControllerAdaptor::new(
        AppState::new(ui.as_weak()),
        Arc::new(RtApiModel::new()),
        tx_api_command.clone(),
        tx_auth_command.clone(),
        tx_asset_command.clone(),
    ));
    adaptor.clone().attach(&ui);
    if let Some(state) = user_state_data_or_null {
        let app_state = adaptor.app_state.lock().unwrap();
        if state.is_expired() {
            app_state
                .set_login_state(LoginState::Unlogged, "登录已过期，请重新登录".into())
                .exec();
            app_state.set_use_auth(String::new(), false).exec();
        } else {
            app_state.set_use_auth(state.account, true).exec();
            let adaptor = adaptor.clone();
            tokio::spawn(async move { adaptor.session_controller.auth().await });
        }
    }
    ui.run()
}

#[tokio::main()]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "desktop-app")]
    if let Some(result) = app::webview::auth::subprocess_webview::launch_if_requested() {
        result?;
    } else {
        let app_window_result = run_app().await;
        app_window_result?;
        println!("[main] APP exited on close requested.");
    }
    
    Ok(())
}
