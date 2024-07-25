/// API用户模型
pub mod api_user_model;
/// API请求处理器，用于接收API命令
pub mod api_worker;
/// AppState，管理UI中状态及其数据映射（Mapping）
pub mod app_state;
/// 资源处理器，用于接收资源命令并请求资源文件及缓存等
pub mod asset_worker;
/// 验证参数相关
pub mod auth;
/// 验证处理器，用于接收处理验证命令并在WebView窗口中进行用户验证/游戏验证
pub mod auth_worker;
/// UI控制器类，用于在Rust运行时和UI组件之间传输数据和执行操作
pub mod controller;
/// 环境（变量）相关
pub mod env;
/// 游戏资源数据类，用于关卡信息显示、立绘定位等
pub mod game_data;
/// 用于在桌面端中处理UI进程和验证网页弹窗进程的通讯
#[cfg(feature = "desktop-app")]
pub mod ipc_auth_comm;
/// APP OTA 功能相关类型
pub mod ota;
/// 启动参数
pub mod program_options;
/// Slint codegen
#[allow(clippy::all)]
pub mod ui;
/// 工具类
pub mod utils;
/// 用于显示网页弹窗或WebView来进行用户验证/游戏验证
#[cfg(feature = "desktop-app")]
pub mod webview;

use self::{
    app_state::AppState,
    controller::{api_user_model::ApiUserModel, UIContext},
};
use api_worker::Worker as ApiWorker;
use arkhost_api::clients::{
    asset::AssetClient,
    common::{ResponseError, UserState, UserStateDataSource, UserStateMemStorage},
    id_server::AuthClient,
};
use asset_worker::AssetWorker;
#[allow(unused)]
use auth_worker::AuthWorker;
use futures::FutureExt;
use std::{
    rc::Rc,
    sync::{Arc, RwLock},
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use ui::*;
use utils::user_state::UserStateDBStore;

pub async fn run() -> Result<(), slint::PlatformError> {
    #[cfg(target_os = "windows")]
    {
        utils::app_user_model::set_to_default_id();
    }

    let user_state = get_user_state();
    let user_state_data_or_null = user_state.read().unwrap().user_state_data();
    let auth_client = create_auth_client(user_state);

    let stop = CancellationToken::new();
    let _guard = stop.clone().drop_guard();

    let mut api_worker = ApiWorker::new(auth_client);
    let (tx_api_command, rx_api_command) = mpsc::channel(32);
    let api_worker_join_handle = tokio::spawn({
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
    let auth_worker_join_handle = tokio::spawn({
        let stop = stop.clone();
        async move {
            auth_worker.run(rx_auth_command, stop).await;
        }
    });

    let asset_client = create_asset_client();
    let mut asset_worker = AssetWorker::new(asset_client);
    let (tx_asset_command, rx_asset_command) = mpsc::channel(32);
    let asset_worker_join_handle = tokio::spawn({
        let stop = stop.clone();
        async move {
            asset_worker.run(rx_asset_command, stop).await;
        }
    });

    let ui = AppWindow::new()?;
    let ui_context = Arc::new(UIContext::new(
        AppState::new(ui.as_weak()),
        Arc::new(ApiUserModel::new()),
        tx_api_command.clone(),
        tx_auth_command.clone(),
        tx_asset_command.clone(),
    ));
    let login_window_ref = Rc::new(std::sync::OnceLock::new());
    let ui_main_thread_context = ui_context.clone().attach(&ui, login_window_ref.clone());
    ui_context.config_controller.sync_to_ui();

    #[cfg(target_os = "windows")]
    let default_webview_installation_found: bool = {
        let ver = utils::webview2::test_installation_ver();
        println!("[app::run] WebView2 installation found: {ver:?}");
        ver.is_some()
    };
    #[cfg(not(target_os = "windows"))]
    // TODO: detect webview installation on other OS
    let default_webview_installation_found = true;

    ui_context.app_state_controller.exec(move |x| {
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
                println!("[app::run] Unknown launch error checking webview availability: {e}");
                false
            }
            Err(e) => {
                println!("[app::run] Unknown error checking webview availability: {e}");
                false
            }
        };
        ui_context.app_state_controller.exec(move |x| {
            x.state_globals(move |s| s.set_has_webview_launch_failure(!webview_launch_success))
        });
    }

    let login_window_context = &ui_main_thread_context.login_window_context;
    let mut show_login_window = false;
    if let Some(state) = user_state_data_or_null {
        login_window_context
            .login_window_state
            .lock()
            .unwrap()
            .set_use_auth(state.account.clone(), true);

        if state.is_expired() {
            let mut login_window_state = login_window_context.login_window_state.lock().unwrap();
            login_window_state
                .set_login_state(LoginState::Unlogged, "登录已过期，请重新登录".into());
            login_window_state.set_use_auth(state.account.clone(), false);
            show_login_window = true;
        } else {
            // 后台获取User Model
            tokio::spawn({
                let ui_context = ui_context.clone();
                async move {
                    ui_context.session_controller.create_user_model().await;
                    ui_context
                        .session_controller
                        .on_post_create_user_model()
                        .await;
                }
            });

            // 后台刷新Token
            tokio::spawn({
                let login_window_state = login_window_context.login_window_state.clone();
                let ui_context = ui_context.clone();
                let state = state.clone();

                async move {
                    let mut refresh_token = ui_context
                        .session_controller
                        .authorize_with_stored_token()
                        .boxed()
                        .fuse();

                    let mut refresh_token_result = None;
                    tokio::select! {
                        result = &mut refresh_token => refresh_token_result = Some(result),
                        _ = tokio::time::sleep(consts::BACKGROUND_REFRESH_TOKEN_TIMEOUT) => {
                            // 后台刷新Token过久，将登录窗口提至前台
                            login_window_state
                                .lock()
                                .unwrap()
                                .set_login_state(LoginState::LoggingIn, "自动登录中……".into());
                            ui_context
                                .app_state_controller
                                .exec(|x| x.exec_in_event_loop(|x| x.invoke_show_login_page()));
                        }
                    }

                    let refresh_token_result = match refresh_token_result {
                        Some(x) => x,
                        None => refresh_token.await, // 继续等待
                    };

                    if let Err(e) = refresh_token_result {
                        println!("[app::run] Refresh token failed: {e}");
                        let mut login_window_state = login_window_state.lock().unwrap();
                        match e.downcast_ref::<ResponseError>() {
                            Some(err_info) if err_info.status_code == 401 => {
                                // token无效
                                login_window_state.set_login_state(
                                    LoginState::Unlogged,
                                    "登录凭据已失效，请重新登录".into(),
                                );
                                login_window_state.set_use_auth(state.account, false);
                            }
                            Some(_) => {
                                // 请求错误，状态码 != 401，如Bad Gateway等
                                login_window_state
                                    .set_login_state(LoginState::Errored, format!("{e}"));
                            }
                            None => {
                                // 其他错误如网络错误
                                login_window_state
                                    .set_login_state(LoginState::Errored, format!("{e}"));
                            }
                        }
                        ui_context
                            .app_state_controller
                            .exec(|x| x.exec_in_event_loop(|x| x.invoke_show_login_page()));
                    } else {
                        println!("[app::run] Refresh token succeeded");
                        let mut login_window_state = login_window_state.lock().unwrap();
                        login_window_state.hide();
                    }
                }
            });
        }
    } else {
        show_login_window = true;
    }

    if show_login_window {
        login_window_context
            .load_login_window()
            .lock()
            .unwrap()
            .show();
    }

    ui.show()?;
    slint::run_event_loop()?;

    // join workers with timeout
    ui_main_thread_context.refresh_game_timer.stop();
    stop.cancel();
    tokio::select! {
        _ = async move {
            tokio::join!(
                join_worker("api_worker", api_worker_join_handle),
                join_worker("auth_worker", auth_worker_join_handle),
                join_worker("asset_worker", asset_worker_join_handle))
        } => {},
        _ = tokio::time::sleep(consts::WORKER_JOIN_TIMEOUT) => {
            utils::notification::toast("可露希尔客户端非正常退出", None, "退出耗时过长，已强制停止。如果频繁遇到请反馈问题。", None);
            let err_info = format!("[app::run] Timed out joining workers! ({:?})", consts::WORKER_JOIN_TIMEOUT);
            println!("{err_info}");
            return Err(slint::PlatformError::Other(err_info));
        }
    };
    Ok(())
}

fn get_user_state() -> Arc<RwLock<dyn UserState>> {
    match env::user_token() {
        Some(token) => Arc::new(RwLock::new(UserStateMemStorage::new(Some(token.into())))),
        None => {
            let mut user_state = UserStateDBStore::new();
            _ = user_state
                .load_from_db()
                .map_err(|e| println!("[app::run] Error loading user state from DB {e}"));
            Arc::new(RwLock::new(user_state))
        }
    }
}

fn create_auth_client(user_state: Arc<RwLock<dyn UserState>>) -> AuthClient {
    use reqwest_retry::policies::ExponentialBackoff;
    use reqwest_retry::RetryTransientMiddleware;
    let client = AuthClient::default_client_builder()
        .use_rustls_tls()
        .gzip(true)
        .brotli(true)
        .connect_timeout(consts::AUTH_CLIENT_CONNECT_TIMEOUT)
        .build()
        .unwrap();

    let retry_policy: ExponentialBackoff =
        ExponentialBackoff::builder().build_with_max_retries(consts::AUTH_CLIENT_MAX_RETRIES);
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
    use utils::cache_control::default_cache_mode_fn;
    use utils::cache_manager::DBCacheManager;

    let client = AssetClient::default_client_builder()
        .use_rustls_tls()
        .gzip(true)
        .brotli(true)
        .connect_timeout(consts::ASSET_CLIENT_CONNECT_TIMEOUT)
        .build()
        .unwrap();

    let retry_policy =
        ExponentialBackoff::builder().build_with_max_retries(consts::ASSET_CLIENT_MAX_RETRIES);
    let client_with_middlewares = reqwest_middleware::ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: DBCacheManager::new(),
            options: HttpCacheOptions {
                cache_mode_fn: Some(default_cache_mode_fn()),
                ..Default::default()
            },
        }))
        .build();

    AssetClient::new(
        env::override_asset_server().unwrap_or(arkhost_api::consts::asset::API_BASE_URL),
        client_with_middlewares,
    )
}

async fn join_worker(worker_name: &str, join_handle: JoinHandle<()>) {
    match join_handle.await {
        Ok(_) => {
            println!("[app::run] Joined worker '{worker_name}'");
        }
        Err(e) => {
            println!("[app::run] Error joining worker '{worker_name}': {e}");
        }
    }
}

mod consts {
    use std::time::Duration;

    pub const AUTH_CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    pub const ASSET_CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    pub const AUTH_CLIENT_MAX_RETRIES: u32 = 3;
    pub const ASSET_CLIENT_MAX_RETRIES: u32 = 2;
    pub const WORKER_JOIN_TIMEOUT: Duration = Duration::from_secs(20);
    pub const BACKGROUND_REFRESH_TOKEN_TIMEOUT: Duration = Duration::from_secs(2);
}
