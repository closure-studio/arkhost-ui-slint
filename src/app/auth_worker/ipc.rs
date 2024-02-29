use std::env;
use std::ffi::OsStr;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

use super::AuthContext;
use super::AuthError;
use super::AuthWorker;
use super::Command;
use crate::app::ipc_auth_comm::AuthenticatorCommError;
use crate::app::ipc_auth_comm::AuthenticatorServerSideChannel;
use crate::app::ipc_auth_comm::{AuthenticatorConnection, AuthenticatorMessage};
use crate::app::webview::auth::{AuthAction, AuthResult};
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use futures_util::future::Fuse;
use futures_util::future::FusedFuture;
use futures_util::FutureExt;
use ipc_channel::ipc::IpcOneShotServer;
use subprocess::ExitStatus;
use tokio::sync::Notify;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

struct AuthProcess {
    connection: AuthenticatorConnection,
    poller: Fuse<BoxFuture<'static, Option<ExitStatus>>>,
}

pub struct IpcAuthWorker {
    auth_process: Option<Arc<Mutex<AuthProcess>>>,
}

#[async_trait]
impl AuthWorker for IpcAuthWorker {
    async fn run(&mut self, mut tx: mpsc::Receiver<AuthContext>, stop: CancellationToken) {
        tokio::select! {
            _ = async {
                while let Some(context) = tx.recv().await {
                    tokio::select! {
                        _ = async {
                            let mut rx_command = context.rx_command;
                            while let Some(cmd) = rx_command.recv().await {
                                self.exec_cmd(cmd).await;
                            }
                        } => {},
                        _ = context.stop.cancelled() => {}
                    }
                    _ = self.set_visible(false).await;
                }
            } => {},
            _ = stop.cancelled() => {}
        }

        if let Some(auth_process) = &self.auth_process {
            auth_process.lock().await.connection.close();
        }
    }
}

impl IpcAuthWorker {
    pub fn new() -> Self {
        Self { auth_process: None }
    }

    async fn exec_cmd(&mut self, cmd: Command) {
        match cmd {
            Command::ArkHostBackground { resp, action } => {
                _ = resp.send(
                    self.auth(
                        AuthAction::ArkHostRestrictedActionBackground {
                            id: "UNUSED".into(),
                            action,
                        },
                        false, // 如果静默验证时出现问题，需要给用户关闭窗口来中止验证，所以不隐藏窗口
                    )
                    .await,
                )
            }
            Command::ArkHostCaptcha { resp, action } => {
                _ = resp.send(
                    self.auth(
                        AuthAction::ArkHostRestrictedActionCaptcha {
                            id: "UNUSED".into(),
                            action,
                        },
                        false,
                    )
                    .await,
                )
            }
            Command::GeeTest {
                resp,
                gt,
                challenge,
            } => {
                _ = resp.send(
                    self.auth(
                        AuthAction::GeeTestAuth {
                            id: gt.clone(),
                            gt,
                            challenge,
                        },
                        false,
                    )
                    .await,
                )
            }
            Command::LaunchAuthenticator { resp } => {
                _ = resp.send(self.try_launch_authenticator().await);
            }
        }
    }

    pub async fn set_visible(&mut self, visible: bool) -> anyhow::Result<()> {
        let auth_process = self.ensure_auth_process().await?;
        let mut auth_process = auth_process.lock().await;
        Ok(auth_process
            .connection
            .send_command(AuthenticatorMessage::SetVisible {
                x: 0.,
                y: 0.,
                visible,
            })
            .await?)
    }

    pub async fn auth(
        &mut self,
        action: AuthAction,
        in_background: bool,
    ) -> anyhow::Result<AuthResult> {
        self.exec_with_auth_process_alive(move |connection| {
            async move {
                connection
                    .send_command(AuthenticatorMessage::SetVisible {
                        x: 250.,
                        y: 250.,
                        visible: !in_background,
                    })
                    .await?;
                let auth_result = connection
                    .send_auth_action(AuthenticatorMessage::PerformAction { action })
                    .await?;
                match auth_result {
                    AuthResult::Failed { .. } => Err(AuthError::Auth(auth_result).into()),
                    _ => Ok(auth_result),
                }
            }
            .boxed()
        })
        .await
    }

    async fn try_launch_authenticator(&mut self) -> Result<(), AuthError> {
        self.exec_with_auth_process_alive(|connection| {
            async {
                let result = connection.send_command(AuthenticatorMessage::Ping).await;
                match result {
                    Ok(()) => Ok(()),
                    Err(AuthenticatorCommError::LaunchWebViewFailed) => {
                        Err(AuthError::LaunchWebView)
                    }
                    Err(e) => Err(AuthError::Launch(e.into())),
                }
            }
            .boxed()
        })
        .await
    }

    async fn exec_with_auth_process_alive<T, E, Op>(&mut self, op: Op) -> Result<T, E>
    where
        for<'a> Op: FnOnce(&'a mut AuthenticatorConnection) -> BoxFuture<'a, Result<T, E>>,
        E: From<AuthError>,
    {
        let auth_process = self
            .ensure_auth_process()
            .await
            .map_err(|e| E::from(AuthError::Launch(e)))?;
        let mut auth_process = auth_process.lock().await;
        let auth_process = auth_process.deref_mut();
        tokio::select! {
            result = async {
                op(&mut auth_process.connection).await
            } => result,
            exit_code = &mut auth_process.poller => Err(AuthError::ProcessExited(format!("{exit_code:?}")).into())
        }
    }

    async fn ensure_auth_process(&mut self) -> anyhow::Result<Arc<Mutex<AuthProcess>>> {
        let alive = match self.auth_process {
            Some(ref auth_process) => {
                self.is_auth_process_alive().await
                    && auth_process
                        .lock()
                        .await
                        .connection
                        .ping(Duration::from_millis(consts::PING_TIMEOUT_MS))
                        .await
            }
            None => false,
        };

        if !alive {
            self.auth_process = Some(self.spawn_auth_process()?);
        }
        Ok(self.auth_process.as_ref().unwrap().clone())
    }

    async fn is_auth_process_alive(&self) -> bool {
        match &self.auth_process {
            Some(auth_process) => !auth_process.lock().await.poller.is_terminated(),
            None => false,
        }
    }

    fn spawn_auth_process(&mut self) -> anyhow::Result<Arc<Mutex<AuthProcess>>> {
        let (ipc_server, ipc_server_name): (
            IpcOneShotServer<AuthenticatorServerSideChannel>,
            String,
        ) = IpcOneShotServer::new().unwrap();

        let current_exe = env::current_exe().unwrap_or_default();
        let mut process = crate::app::utils::subprocess::spawn_executable(
            current_exe.as_os_str(),
            &[
                current_exe.as_os_str(),
                OsStr::new("--launch-webview"),
                OsStr::new("--account"),
                OsStr::new("UNUSED"),
                OsStr::new("--ipc"),
                OsStr::new(&ipc_server_name),
            ],
            None,
            true,
            None,
            None,
        )?;

        match AuthenticatorConnection::accept(ipc_server) {
            Ok(connection) => Ok(Arc::new(Mutex::new(AuthProcess {
                connection,
                poller: async move {
                    let notify = Arc::new(Notify::new());
                    let thread = std::thread::spawn({
                        let notify = notify.clone();
                        move || {
                            let exit_status = process.wait().ok();
                            println!(
                                "[IpcAuthWorker] Auth process exited with status {exit_status:?}"
                            );
                            notify.notify_one();
                            exit_status
                        }
                    });
                    notify.notified().await;
                    thread.join().unwrap_or(None)
                }
                .boxed()
                .fuse(),
            }))),
            Err(e) => {
                _ = process.terminate();
                Err(e)
            }
        }
    }
}

mod consts {
    pub const PING_TIMEOUT_MS: u64 = 3000;
}
