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

use std::sync::{Arc, RwLock};

use arkhost_api::clients::{
    asset::AssetClient,
    common::{UserState, UserStateDataSource},
    id_server::AuthClient,
};
use tokio::sync::{mpsc, oneshot};

use tokio_util::sync::CancellationToken;
use utils::user_state::UserStateDBStore;

use api_worker::Worker as ApiWorker;
use asset_worker::AssetWorker;
use ui::*;

#[allow(unused)]
use auth_worker::AuthWorker;

use self::{
    app_state::AppState,
    controller::{rt_api_model::RtApiModel, ControllerAdaptor},
};

pub async fn run() -> Result<(), slint::PlatformError> {
    #[cfg(target_os = "windows")]
    {
        utils::app_user_model::set_to_default_id();
    }

    let mut user_state = UserStateDBStore::new();
    user_state.load_from_db();
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
    let mut auth_worker = auth_worker::ipc::IpcAuthWorker::new();
    #[cfg(feature = "android-app")]
    let mut auth_worker = auth_worker::geetest_sdk::GeeTestSdkAuthWorker::new();
    #[cfg(all(not(feature = "desktop-app"), not(feature = "android-app")))]
    let mut auth_worker = compile_error!("No AuthWorker implementation available");

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

    #[cfg(target_os = "windows")]
    let default_webview_installation_found = {
        let ver = utils::webview2::test_installation_ver();
        println!("[WebView2] installation found: {ver:?}");
        ver.is_some()
    };
    #[cfg(not(target_os = "windows"))]
    // TODO: detect webview installation on other OS
    let default_webview_installation_found = true;

    adaptor.app_state_controller.exec(move |x| {
        x.state_globals(move |s| {
            #[cfg(target_os = "windows")]
            s.set_default_webview_installation_type(ui::WebViewType::MicrosoftEdgeWebView2);
            s.set_has_default_webview_installation(default_webview_installation_found);
        })
    });
    if !default_webview_installation_found {
        use auth_worker::{AuthContext, AuthError};
        use futures::TryFutureExt;
        let (tx_command, rx_command) = mpsc::channel(1);
        let stop = CancellationToken::new();
        let _guard = stop.clone().drop_guard();
        let result = tx_auth_command
            .send(AuthContext { rx_command, stop })
            .map_err(anyhow::Error::from)
            .and_then(|_| {
                let (resp, rx_launch_result) = oneshot::channel();
                tx_command
                    .send(auth_worker::Command::LaunchAuthenticator { resp })
                    .map_err(anyhow::Error::from)
                    .and_then(|_| rx_launch_result.map_err(anyhow::Error::from))
            })
            .await;
        let webview_launch_success = match result {
            Ok(Ok(())) => true,
            Ok(Err(AuthError::LaunchWebView)) | Ok(Err(AuthError::ProcessExited(_))) => false,
            Ok(Err(e)) => {
                println!("Unknown launch error checking webview availability: {e}");
                false
            }
            Err(e) => {
                println!("Unknown error checking webview availability: {e}");
                false
            }
        };
        adaptor.app_state_controller.exec(move |x| {
            x.state_globals(move |s| s.set_has_webview_launch_failure(!webview_launch_success))
        });
    }

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

fn create_auth_client(user_state: Arc<RwLock<dyn UserState>>) -> AuthClient {
    use reqwest_retry::policies::ExponentialBackoff;
    use reqwest_retry::RetryTransientMiddleware;
    use std::time::Duration;
    let client = AuthClient::default_client_builder()
        .use_rustls_tls()
        .gzip(true)
        .brotli(true)
        .connect_timeout(Duration::from_secs(8))
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
    use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
    use http_cache_reqwest::Cache;
    use reqwest_retry::policies::ExponentialBackoff;
    use reqwest_retry::RetryTransientMiddleware;
    use std::time::Duration;
    use utils::cache_manager::DBCacheManager;

    let client = AssetClient::default_client_builder()
        .use_rustls_tls()
        .gzip(true)
        .brotli(true)
        .connect_timeout(Duration::from_secs(8))
        .build()
        .unwrap();

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client_with_middlewares = reqwest_middleware::ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: DBCacheManager::new(),
            options: HttpCacheOptions {
                cache_mode_fn: Some(Arc::new(|req| {
                    if matches!(req.method.as_str(), "HEAD" | "OPTIONS") {
                        return CacheMode::NoStore;
                    }

                    // TODO: 其他方式识别资源文件类型（MIME type等）
                    if req.uri.path().ends_with(".webp") {
                        return CacheMode::ForceCache;
                    }
                    let matches_ota_file = {
                        // OTA 更新文件URL： http://asset.server.com/foo/bar.txt/{hash}
                        let mut split = req.uri.path().rsplitn(2, '/');
                        !matches!(
                            (split.next(), split.next()), 
                                (Some(hash_versioned_file), Some(hash_version_dir)) if
                                    (hash_version_dir.ends_with(".exe")
                                    || hash_versioned_file.ends_with(".bspatch")))
                        // TODO: 其他方式识别OTA更新文件
                    };
                    if matches_ota_file {
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
        env::override_asset_server().unwrap_or(arkhost_api::consts::asset::API_BASE_URL),
        client_with_middlewares,
    )
}
