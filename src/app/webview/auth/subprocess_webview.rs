use super::super::auth;
use crate::app::ipc_auth_comm::AuthenticatorMessage;
use crate::app::utils::data_dir;
use crate::app::webview::auth::consts;
use anyhow::anyhow;
use argh::FromArgs;
use ipc_channel::ipc;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use std::rc::Rc;
use std::thread;
use std::time::Duration;
use thiserror::Error;
use winit::window::WindowLevel;
use winit::{
    dpi::LogicalPosition,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::WindowButtons,
};
use wry::{WebContext, WebViewBuilder};

#[derive(Error, Debug)]
pub enum ChildProcessAuthenticatorError {
    #[error("invalid launch args: {launch_args_dbg_str}")]
    InvalidLaunchArgs { launch_args_dbg_str: String },
}

#[derive(Debug, Clone, FromArgs)]
/// 用于用户验证的 WebView 启动参数，如果使用 WebView 启动，则不打开主UI界面
pub struct LaunchArgs {
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

struct Listener {
    event_loop_proxy: EventLoopProxy<AuthenticatorMessage>,
}

impl auth::AuthListener for Listener {
    fn on_result(&self, result: auth::AuthResult) {
        let res = self
            .event_loop_proxy
            .send_event(AuthenticatorMessage::Result { result });

        if let Err(e) = res {
            eprintln!("[WebViewSubprocess] Error sending event to EventLoop: {e}")
        }
    }
}

pub fn launch(args: LaunchArgs) -> anyhow::Result<()> {
    let server_name = args.ipc.unwrap();
    let (tx_host, rx_host) = connect_to_host_process(server_name)?;
    match {
        let tx_host = tx_host.clone();
        launch_webview(tx_host, rx_host, args.account.unwrap())
    } {
        Ok(_) => Ok(()),
        Err(e) => {
            _ = tx_host.send(AuthenticatorMessage::LaunchWebViewFailed);
            Err(e)
        }
    }
}

pub fn launch_if_requested() -> Option<anyhow::Result<()>> {
    let launch_args: LaunchArgs = argh::from_env();

    match launch_args {
        LaunchArgs {
            launch_webview: None,
            ..
        } => None,
        LaunchArgs {
            launch_webview: Some(true),
            account: Some(_),
            ipc: Some(_),
        } => Some(launch(launch_args)),
        _ => Some(Err(ChildProcessAuthenticatorError::InvalidLaunchArgs {
            launch_args_dbg_str: format!("{launch_args:?}"),
        }
        .into())),
    }
}

type TxHost = IpcSender<AuthenticatorMessage>;
type RxHost = IpcReceiver<AuthenticatorMessage>;

fn connect_to_host_process(server_name: String) -> anyhow::Result<(TxHost, RxHost)> {
    let (rx_host_sender, rx_host): (
        IpcSender<AuthenticatorMessage>,
        IpcReceiver<AuthenticatorMessage>,
    ) = ipc::channel()?;
    let (tx_host_sender, tx_host_receiver): (
        IpcSender<IpcSender<AuthenticatorMessage>>,
        IpcReceiver<IpcSender<AuthenticatorMessage>>,
    ) = ipc::channel()?;
    println!("[WebViewSubprocess] Sending inverse side senders to host {server_name}");

    let mut retries = 3;
    loop {
        match IpcSender::connect(server_name.clone()) {
            Ok(sender) => break Ok(sender),
            Err(e) => {
                println!("[WebViewSubprocess] failed attempting to connect, retrying...\n{e}");
            }
        }

        retries -= 1;
        if retries > 0 {
            thread::sleep(Duration::from_secs(1))
        } else {
            break Err(anyhow!("all attempts to connect failed"));
        }
    }?
    .send((rx_host_sender, tx_host_sender))?;

    println!("[WebViewSubprocess] Receiving host TX receiver from host");
    let tx_host: IpcSender<AuthenticatorMessage> = tx_host_receiver.recv()?;
    Ok((tx_host, rx_host))
}

fn launch_webview(tx_host: TxHost, rx_host: RxHost, account: String) -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::<AuthenticatorMessage>::with_user_event()
        .build()
        .unwrap();

    let window = winit::window::WindowBuilder::new()
        .with_visible(false)
        .with_title("Closure Studio - 用户验证")
        .with_enabled_buttons(WindowButtons::MINIMIZE | WindowButtons::CLOSE)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .build(&event_loop)
        .unwrap();

    println!("[WebViewSubprocess] Launching authenticator WebView");
    let authenticator = auth::Authenticator::new(
        auth::AuthParams::ArkHostAuth {
            user: account,
        },
        Rc::new(Box::new(Listener {
            event_loop_proxy: event_loop.create_proxy(),
        })),
    );

    let user_data_dir = data_dir::data_dir().join(consts::WEBVIEW_USER_DATA_DIR);
    println!(
        "[WebViewSubprocess] User data directory: {}",
        user_data_dir.display()
    );
    let mut web_context = WebContext::new(Some(user_data_dir));

    let webview = authenticator
        .build_webview(WebViewBuilder::new(&window).with_web_context(&mut web_context))?
        .build()?;
    authenticator
        .webview
        .write()
        .unwrap()
        .set_webview(Rc::new(webview));

    let proxy = event_loop.create_proxy();

    thread::spawn(move || {
        let mut close_requested = false;
        loop {
            match rx_host.recv() {
                Ok(event) => {
                    if let AuthenticatorMessage::CloseRequested = event {
                        close_requested = true;
                    }

                    if let Err(e) = proxy.send_event(event) {
                        println!("[WebViewSubprocess] Closing Authenticator on EventLoop send failed: {e:?}");
                        break;
                    }
                }
                Err(e) => {
                    match e {
                        ipc::IpcError::Disconnected => {
                            println!("[WebViewSubprocess] Closing Authenticator on host_tx_receiver Disconnected")
                        }
                        _ => println!(
                            "[WebViewSubprocess] Closing Authenticator on host_tx_receiver recv failed: {e:?}"
                        ),
                    }
                    break;
                }
            }
        }
        if !close_requested {
            println!("[WebViewSubprocess] Closing Authenticator from recv thread");
            _ = proxy.send_event(AuthenticatorMessage::CloseRequested);
        }
    });
    event_loop.run(move |event, evl| {
        evl.set_control_flow(ControlFlow::Wait);
        if let Event::UserEvent(ev) = &event {
            println!("[WebViewSubprocess] AuthenticatorEvent {ev:?}");
        }

        match event {
            Event::UserEvent(ev) => match ev {
                AuthenticatorMessage::SetVisible { x, y, visible } => {
                    window.set_visible(visible);
                    if visible {
                        window.set_outer_position(LogicalPosition::new(x, y));
                    }
                    _ = tx_host.send(AuthenticatorMessage::Acknowledged);
                }
                AuthenticatorMessage::PerformAction { action } => {
                    let preform_res = authenticator.auth_resolver.preform(action.clone());
                    if let Err(err) = preform_res {
                        eprintln!("[WebViewSubprocess] Error preforming auth action: '{action:?}'; Err: {err}");
                    }
                    _ = tx_host.send(AuthenticatorMessage::Acknowledged);
                }
                AuthenticatorMessage::Result { .. } => {
                    let send_res = tx_host.send(ev);
                    if let Err(e) = send_res {
                        eprintln!("[WebViewSubprocess] Unable to send auth result, Err: {e}");
                    }
                }
                AuthenticatorMessage::CloseRequested => {
                    _ = tx_host.send(AuthenticatorMessage::Acknowledged);
                    _ = tx_host.send(AuthenticatorMessage::Closed);
                    evl.exit();
                }
                AuthenticatorMessage::ReloadRequested => {
                    let res = authenticator.reload();
                    if let Err(e) = res {
                        eprintln!("[WebViewSubprocess] Unable to reload auth page, Err: {e}");
                    }
                    _ = tx_host.send(AuthenticatorMessage::Acknowledged);
                }
                _ => eprintln!("[WebViewSubprocess] Error listening AuthenticatorEvent: handler not implemented for {ev:?}"),
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                _ = tx_host.send(AuthenticatorMessage::Closed);
                evl.exit();
            }
            _ => {}
        }
    })?;

    Ok(())
}
