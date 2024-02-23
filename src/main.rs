#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;

use anyhow::bail;
use app::app_state::AppState;
use app::asset_worker::AssetWorker;
#[cfg(feature = "desktop-app")]
use app::auth_worker::ipc::IpcAuthWorker;

use app::auth_worker::{self, AuthContext, AuthError, AuthWorker};
use app::controller::rt_api_model::RtApiModel;
use app::program_options::LaunchArgs;
use app::ui::*;
use app::utils::cache_manager::CACacheManager;
use app::utils::data_dir::data_dir;
use app::utils::subprocess::spawn_executable;
use app::utils::user_state::{UserStateFileStorage, UserStateFileStoreSetting};
use app::{api_worker::Worker as ApiWorker, controller::ControllerAdaptor};
use arkhost_api::clients::asset::AssetClient;
use arkhost_api::clients::common::UserState;
use arkhost_api::clients::{common::UserStateDataSource, id_server::AuthClient};
use futures_util::TryFutureExt;
use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::env;
use std::ffi::OsStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::{self, sync::mpsc};
use tokio_util::sync::CancellationToken;

fn create_auth_client(user_state: Arc<RwLock<dyn UserState>>) -> AuthClient {
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
    #[cfg(target_os = "windows")]
    {
        app::utils::app_user_model::set_to_default_id();
    }

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

    #[cfg(target_os = "windows")]
    let default_webview_installation_found = {
        let ver = app::utils::webview2::test_installation_ver();
        println!("[WebView2] installation found: {ver:?}");
        ver.is_some()
    };
    #[cfg(not(target_os = "windows"))]
    // TODO: detect webview installation on other OS
    let default_webview_installation_found = true;

    adaptor.app_state_controller.exec(move |x| {
        x.state_globals(move |s| {
            #[cfg(target_os = "windows")]
            s.set_default_webview_installation_type(app::ui::WebViewType::MicrosoftEdgeWebView2);
            s.set_has_default_webview_installation(default_webview_installation_found);
        })
    });
    if !default_webview_installation_found {
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

async fn launch_app_window_if_requested(
    launch_args: &LaunchArgs,
) -> Option<Result<(), slint::PlatformError>> {
    if let Some(true) = launch_args.launch_app_window {
        // 目前只支持同时使用 --launch-app-window --attach-console 显示 AppWindow 进程的 Stdout
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::Foundation::GetLastError;
            use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
            if let Some(true) = launch_args.attach_console {
                let result = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
                if result != 0 {
                    println!("[launch_app_window_if_requested] AttachConsole(ATTACH_PARENT_PROCESS) success ");
                } else {
                    println!("[launch_app_window_if_requested] Error attaching to console: {:#x}", unsafe {
                        GetLastError()
                    });
                }
            }
        }

        Some(run_app().await)
    } else {
        None
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let launch_args: LaunchArgs = argh::from_env();
    if let (Some(true), Some(true)) = (&launch_args.launch_app_window, &launch_args.launch_webview)
    {
        bail!("invalid parameters");
    }

    #[cfg(feature = "desktop-app")]
    if let Some(result) = app::webview::auth::subprocess_webview::launch_if_requested(&launch_args)
    {
        result?;
    } else if let Some(result) = launch_app_window_if_requested(&launch_args).await {
        result?;
    } else {
        println!("[main] Spawning AppWindow process");
        let current_exe = env::current_exe().unwrap_or_default();
        let mut app_window = spawn_executable(
            current_exe.as_os_str(),
            &[current_exe.as_os_str(), OsStr::new("--launch-app-window")],
            None,
            true,
        )?;

        let exit_status = app_window.wait()?;
        println!("[main] AppWindow process exited with status '{exit_status:?}'");
    }

    Ok(())
}
