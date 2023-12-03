use super::super::auth;
use crate::app::ipc::AuthenticatorMessage;
use argh::FromArgs;
use ipc_channel::ipc;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use std::sync::Arc;
use std::thread;
use thiserror::Error;
use winit::{
    dpi::LogicalPosition,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::WindowButtons,
};
use wry::WebViewBuilder;

#[derive(Error, Debug)]
pub enum ChildProcessAuthenticatorError {
    #[error("invalid launch args: {launch_args_dbg_str}")]
    InvalidLaunchArgs { launch_args_dbg_str: String },
}

#[derive(Debug, Clone, FromArgs)]
/// 用于用户验证的WebView启动参数，如果使用WebView启动，则不打开主UI界面
pub struct LaunchArgs {
    #[argh(switch)]
    /// 是否启动WebView
    pub launch_webview: Option<bool>,

    #[argh(option)]
    /// 要验证的账号
    pub account: Option<String>,

    #[argh(option)]
    /// 父进程的IPC Server（发送端）名称
    pub host_tx: Option<String>,

    #[argh(option)]
    /// 父进程的IPC Server（接收端）名称
    pub host_rx: Option<String>,
}

pub fn launch(args: LaunchArgs) -> anyhow::Result<()> {
    struct Listener {
        event_loop_proxy: EventLoopProxy<AuthenticatorMessage>,
    }
    impl auth::AuthListener for Listener {
        fn on_result(&self, result: auth::AuthResult) {
            let res = self
                .event_loop_proxy
                .send_event(AuthenticatorMessage::Result { result });

            if let Err(e) = res {
                eprintln!(
                    "[WebViewSubprocess] Error sending event to EventLoop: {}",
                    e
                )
            }
        }
    }

    let (host_tx_inverse_sender, host_tx_receiver): (
        IpcSender<AuthenticatorMessage>,
        IpcReceiver<AuthenticatorMessage>,
    ) = ipc::channel()?;
    println!(
        "[WebViewSubprocess] Connecting to host TX {}",
        args.host_tx.as_ref().unwrap()
    );
    IpcSender::connect(args.host_tx.unwrap())?.send(host_tx_inverse_sender)?;
    println!(
        "[WebViewSubprocess] Connecting to host RX {}",
        args.host_rx.as_ref().unwrap()
    );
    let host_rx_sender: IpcSender<AuthenticatorMessage> =
        IpcSender::connect(args.host_rx.unwrap())?;
    host_rx_sender.send(AuthenticatorMessage::Acknowledged)?;

    let event_loop = EventLoopBuilder::<AuthenticatorMessage>::with_user_event()
        .build()
        .unwrap();

    let window = winit::window::WindowBuilder::new()
        .with_visible(false)
        .with_title("Closure Studio - 用户验证")
        .with_enabled_buttons(WindowButtons::MINIMIZE | WindowButtons::CLOSE)
        .build(&event_loop)
        .unwrap();

    println!("[WebViewSubprocess] Launching authenticator WebView");
    let authenticator = auth::Authenticator::new(
        auth::AuthParams::ArkHostAuth {
            user: args.account.unwrap(),
        },
        Arc::new(Box::new(Listener {
            event_loop_proxy: event_loop.create_proxy(),
        })),
    );

    let webview = authenticator
        .build_webview(WebViewBuilder::new(&window))?
        .build()?;
    authenticator
        .webview
        .write()
        .unwrap()
        .set_webview(Arc::new(webview));

    let proxy = event_loop.create_proxy();

    thread::spawn(move || {
        let mut close_requested = false;
        loop {
            match host_tx_receiver.recv() {
                Ok(event) => {
                    if let AuthenticatorMessage::CloseRequested = event {
                        close_requested = true;
                    }

                    if let Err(e) = proxy.send_event(event) {
                        println!("[WebViewSubprocess] Closing Authenticator on EventLoop send failed: {:?}", e);
                        break;
                    }
                }
                Err(e) => {
                    match e {
                        ipc::IpcError::Disconnected => {
                            println!("[WebViewSubprocess] Closing Authenticator on host_tx_receiver Disconnected")
                        }
                        _ => println!(
                            "[WebViewSubprocess] Closing Authenticator on host_tx_receiver recv failed: {:?}",
                            e
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
        if let Event::UserEvent(e) = &event {
            println!("[WebViewSubprocess] AuthenticatorEvent {:?}", e);
        }

        match event {
            Event::UserEvent(ev) => match ev {
                AuthenticatorMessage::SetVisible { x, y, visible } => {
                    window.set_visible(visible);
                    if visible {
                        window.set_outer_position(LogicalPosition::new(x, y));
                    }
                    _ = host_rx_sender.send(AuthenticatorMessage::Acknowledged);
                }
                AuthenticatorMessage::PerformAction { action } => {
                    let preform_res = authenticator.auth_resolver.preform(action.clone());
                    if let Err(err) = preform_res {
                        eprintln!("[WebViewSubprocess] Error preforming auth action: '{:?}'; Err: {}", action, err);
                    }
                    _ = host_rx_sender.send(AuthenticatorMessage::Acknowledged);
                }
                AuthenticatorMessage::Result { .. } => {
                    let send_res = host_rx_sender.send(ev);
                    if let Err(e) = send_res {
                        eprintln!("[WebViewSubprocess] Unable to send auth result, Err: {}", e);
                    }
                }
                AuthenticatorMessage::CloseRequested => {
                    _ = host_rx_sender.send(AuthenticatorMessage::Acknowledged);
                    _ = host_rx_sender.send(AuthenticatorMessage::Closed);
                    evl.exit();
                }
                AuthenticatorMessage::ReloadRequested => {
                    let res = authenticator.reload();
                    if let Err(e) = res {
                        eprintln!("[WebViewSubprocess] Unable to reload auth page, Err: {}", e);
                    }
                    _ = host_rx_sender.send(AuthenticatorMessage::Acknowledged);
                }
                #[allow(unreachable_patterns)]
                _ => eprintln!(
                    "[WebViewSubprocess] Error listening AuthenticatorEvent: handler not implemented for {:?}",
                    ev
                ),
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                _ = host_rx_sender.send(AuthenticatorMessage::Closed);
                evl.exit();
            }
            _ => {}
        }
    })?;

    Ok(())
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
            host_tx: Some(_),
            host_rx: Some(_),
        } => Some(launch(launch_args)),
        _ => Some(Err(ChildProcessAuthenticatorError::InvalidLaunchArgs {
            launch_args_dbg_str: format!("{:?}", launch_args),
        }
        .into())),
    }
}
