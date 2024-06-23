use super::super::auth;
use super::Authenticator;
use crate::app;
use crate::app::ipc_auth_comm::AuthenticatorMessage;
use crate::app::program_options::LaunchWebViewArgs;
use crate::app::utils::data_dir;
use crate::app::webview::auth::consts;
use anyhow::anyhow;
use ipc_channel::ipc;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use std::rc::Rc;
use std::thread;
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::event_loop::EventLoop;
use winit::window::WindowLevel;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::{Window, WindowButtons},
};
use wry::{WebContext, WebViewBuilder};

struct Listener {
    event_loop_proxy: EventLoopProxy<AuthenticatorMessage>,
}

impl auth::AuthListener for Listener {
    fn on_result(&self, result: auth::AuthResult) {
        let res = self
            .event_loop_proxy
            .send_event(AuthenticatorMessage::Result { result });

        if let Err(e) = res {
            println!("[WebViewSubprocess] Error sending event to EventLoop: {e}")
        }
    }
}

struct AuthenticatorApp {
    window: Option<Window>,
    authenticator: Authenticator,
    tx_host: TxHost,
    visible: bool,
}

impl AuthenticatorApp {
    fn new(tx_host: TxHost, authenticator: Authenticator) -> Self {
        Self {
            window: None,
            tx_host,
            authenticator,
            visible: false,
        }
    }
}

impl ApplicationHandler<AuthenticatorMessage> for AuthenticatorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // create window
        let window_attributes = Window::default_attributes()
            .with_visible(self.visible)
            .with_title("Closure Studio - 用户验证")
            .with_enabled_buttons(WindowButtons::MINIMIZE | WindowButtons::CLOSE)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize {
                width: 360,
                height: 480,
            });

        let window = event_loop.create_window(window_attributes).unwrap();

        // create webview
        let user_data_dir = data_dir::data_dir().join(consts::WEBVIEW_USER_DATA_DIR);
        println!(
            "[WebViewSubprocess] User data directory: {}",
            user_data_dir.display()
        );
        let mut web_context = WebContext::new(Some(user_data_dir));

        let webview = self
            .authenticator
            .build_webview(WebViewBuilder::new(&window).with_web_context(&mut web_context))
            .build()
            .unwrap();
        self.authenticator
            .webview
            .write()
            .unwrap()
            .set_webview(Rc::new(webview));

        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        evl: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        evl.set_control_flow(ControlFlow::Wait);
        match event {
            WindowEvent::CloseRequested => {
                evl.exit();
            }
            _ => {}
        }
    }

    fn user_event(&mut self, evl: &ActiveEventLoop, ev: AuthenticatorMessage) {
        evl.set_control_flow(ControlFlow::Wait);
        println!("[WebViewSubprocess] AuthenticatorEvent {ev:?}");
        match ev {
            AuthenticatorMessage::Ping => {
                _ = self.tx_host.send(AuthenticatorMessage::Acknowledged);
            }
            AuthenticatorMessage::SetVisible { x, y, visible } => {
                self.window.as_ref().unwrap().set_visible(visible);
                if visible {
                    self.window.as_ref().unwrap().set_outer_position(LogicalPosition::new(x, y));
                }
                _ = self.tx_host.send(AuthenticatorMessage::Acknowledged);
            }
            AuthenticatorMessage::PerformAction { action } => {
                let preform_res = self.authenticator.auth_resolver.preform(action.clone());
                if let Err(err) = preform_res {
                    println!("[WebViewSubprocess] Error preforming auth action: '{action:?}'; Err: {err}");
                }
                _ = self.tx_host.send(AuthenticatorMessage::Acknowledged);
            }
            AuthenticatorMessage::Result { .. } => {
                let send_res = self.tx_host.send(ev);
                if let Err(e) = send_res {
                    println!("[WebViewSubprocess] Unable to send auth result, Err: {e}");
                }
            }
            AuthenticatorMessage::CloseRequested => {
                _ = self.tx_host.send(AuthenticatorMessage::Acknowledged);
                _ = self.tx_host.send(AuthenticatorMessage::Closed);
                evl.exit();
            }
            AuthenticatorMessage::ReloadRequested => {
                let res = self.authenticator.reload();
                if let Err(e) = res {
                    println!("[WebViewSubprocess] Unable to reload auth page, Err: {e}");
                }
                _ = self.tx_host.send(AuthenticatorMessage::Acknowledged);
            }
            _ => println!("[WebViewSubprocess] Error listening AuthenticatorEvent: handler not implemented for {ev:?}"),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        _ = self.tx_host.send(AuthenticatorMessage::Closed);
    }
}

pub fn launch(launch_args: &LaunchWebViewArgs) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    app::utils::app_user_model::set_to_authenticator_id();

    let server_name = launch_args.ipc.to_owned();
    let (tx_host, rx_host) = connect_to_host_process(server_name)?;
    match launch_webview(tx_host.clone(), rx_host, launch_args.account.to_owned()) {
        Ok(_) => Ok(()),
        Err(e) => {
            _ = tx_host.send(AuthenticatorMessage::LaunchWebViewFailed);
            Err(e)
        }
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
    let event_loop = EventLoop::<AuthenticatorMessage>::with_user_event()
        .build()
        .unwrap();

    println!("[WebViewSubprocess] Launching authenticator WebView");
    let authenticator = auth::Authenticator::new(
        auth::AuthParams::ArkHostAuth { user: account },
        Rc::new(Box::new(Listener {
            event_loop_proxy: event_loop.create_proxy(),
        })),
    );

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
    let mut app = AuthenticatorApp::new(tx_host, authenticator);
    event_loop.run_app(&mut app)?;

    Ok(())
}
