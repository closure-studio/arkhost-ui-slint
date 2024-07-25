#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
#[cfg(feature = "desktop-app")]
mod desktop_utils;

use app::program_options::{LaunchAppWindowArgs, LaunchArgs, LaunchSpec};
#[cfg(feature = "desktop-app")]
use app::utils::subprocess::spawn_executable;
#[cfg(feature = "desktop-app")]
use desktop_utils::*;

use log::info;

async fn launch_app_window(_launch_args: &LaunchAppWindowArgs) -> Result<(), slint::PlatformError> {
    app::run().await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let launch_args: LaunchArgs = argh::from_env();
    env_logger::init();
    let _cleanup_guard = app::utils::db::CleanupGuard::new();

    match &launch_args.launch_spec {
        // Bootstrap
        None => {
            #[cfg(feature = "desktop-app")]
            let _instance = {
                let instance: single_instance::SingleInstance =
                    single_instance::SingleInstance::new("arkhost-ui-slint-single-instance")
                        .unwrap();
                if !instance.is_single() {
                    on_duplicated_instance();
                    anyhow::bail!("duplicated instance");
                }

                instance
            };

            #[cfg(any(
                feature = "desktop-app",
                all(feature = "desktop-app", debug_assertions)
            ))]
            if matches!(launch_args.attach_console, Some(true)) || app::env::attach_console() {
                attach_console();
            } else {
                alloc_console();
                show_console(false);
            }

            info!(
                "\n### arkhost-ui-slint {} ###\n",
                app::utils::app_metadata::CARGO_PKG_VERSION.unwrap_or("not found")
            );

            app::utils::db::handle_self_delete(true);

            #[cfg(feature = "desktop-app")]
            {
                let current_exe = std::env::current_exe().unwrap_or_default();

                let mut env = vec![];
                if let Some(true) = launch_args.attach_console {
                    env.push((app::env::consts::ATTACH_CONSOLE.into(), "1".into()));
                }
                if let Some(true) = launch_args.force_update {
                    env.push((app::env::consts::FORCE_UPDATE.into(), "1".into()));
                }
                if let Some(ref asset_server) = launch_args.asset_server {
                    env.push((
                        app::env::consts::OVERRIDE_ASSET_SERVER.into(),
                        asset_server.into(),
                    ))
                }
                if let Some(ref user_token) = launch_args.user_token {
                    env.push((app::env::consts::USER_TOKEN.into(), user_token.into()))
                }

                let mut app_window = spawn_executable(
                    current_exe.as_os_str(),
                    &[std::ffi::OsStr::new("app")],
                    Some(env),
                    true,
                    None,
                    None,
                )?;

                let exit_status = app_window.wait().await?;
                info!("\n### AppWindow process exited with status '{exit_status}' ###\n");

                if exit_status.success() {
                    if let Err(e) = update_client_if_exist().await {
                        show_crash_window(&format!("{exit_status}"), &format!("更新失败\n{e}"));
                    }
                } else {
                    show_crash_window(&format!("{exit_status}"), "主窗口非正常退出");
                }
            }
        }
        Some(LaunchSpec::AppWindow(launch_args)) => {
            #[cfg(any(
                feature = "desktop-app",
                all(feature = "desktop-app", debug_assertions)
            ))]
            attach_console();
            launch_app_window(launch_args).await?;
        }
        #[allow(unused)]
        Some(LaunchSpec::WebView(launch_args)) => {
            #[cfg(any(
                feature = "desktop-app",
                all(feature = "desktop-app", debug_assertions)
            ))]
            attach_console();
            #[cfg(feature = "desktop-app")]
            app::webview::auth::subprocess_webview::launch(launch_args)?;
        }
    }

    Ok(())
}
