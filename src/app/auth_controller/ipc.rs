use std::ffi::OsStr;
use std::sync::Arc;

use super::AuthController;
use super::Command;
use crate::app::ipc::{AuthenticatorConnection, AuthenticatorMessage};
use crate::app::webview::auth::{AuthAction, AuthResult};
use anyhow::anyhow;
use async_trait::async_trait;
use ipc_channel::ipc::IpcOneShotServer;
use ipc_channel::ipc::IpcSender;
use subprocess::Popen;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

struct AuthProcess {
    process: Popen,
    connection: AuthenticatorConnection,
}

pub struct IpcAuthController {
    auth_process: Option<Arc<Mutex<AuthProcess>>>,
}

#[async_trait]
impl AuthController for IpcAuthController {
    async fn run(&mut self, mut tx: mpsc::Receiver<super::Command>, stop: CancellationToken) {
        tokio::select! {
            _ = async {
                while let Some(cmd) = tx.recv().await {
                    match cmd {
                        Command::AuthArkHostBackground { resp, action } => {
                            _ = resp.send(
                                self.auth(
                                    AuthAction::ArkHostRestrictedActionBackground {
                                        id: "UNUSED".into(),
                                        action,
                                    },
                                    false,
                                )
                                .await,
                            )
                        }
                        Command::AuthArkHostCaptcha { resp, action } => {
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
                        },
                        Command::AuthGeeTest { resp, gt, challenge } => {
                            _ = resp.send(
                                self.auth(AuthAction::GeeTestAuth { id: gt.clone(), gt, challenge }, false).await
                            )
                        }
                        Command::HideWindow {} => {
                            _ = self.set_visible(false).await;
                        }
                    }
                }
            } => {},
            _ = stop.cancelled() => {}
        }

        if let Some(auth_process) = &self.auth_process {
            auth_process.lock().await.connection.close();
        }
    }
}

impl IpcAuthController {
    pub fn new() -> Self {
        Self { auth_process: None }
    }

    pub async fn set_visible(&mut self, visible: bool) -> anyhow::Result<()> {
        let auth_process = self.ensure_auth_process().await?;
        let mut auth_process = auth_process.lock().await;
        auth_process
            .connection
            .send_command(AuthenticatorMessage::SetVisible {
                x: 0.,
                y: 0.,
                visible,
            })
            .await
    }

    pub async fn auth(
        &mut self,
        action: AuthAction,
        background: bool,
    ) -> anyhow::Result<AuthResult> {
        let auth_process = self.ensure_auth_process().await?;
        let mut auth_process = auth_process.lock().await;
        auth_process
            .connection
            .send_command(AuthenticatorMessage::SetVisible {
                x: 250.,
                y: 250.,
                visible: !background,
            })
            .await?;
        let auth_result = auth_process
            .connection
            .send_auth_action(AuthenticatorMessage::PerformAction { action })
            .await?;
        match auth_result {
            AuthResult::Failed { .. } => Err(anyhow!("auth failed: {auth_result:?}")),
            _ => Ok(auth_result),
        }
    }

    async fn ensure_auth_process(&mut self) -> anyhow::Result<Arc<Mutex<AuthProcess>>> {
        if self.auth_process.is_none() || !self.is_auth_process_alive().await {
            self.auth_process = Some(self.spawn_auth_process()?);
        }
        Ok(self.auth_process.as_ref().unwrap().clone())
    }

    async fn is_auth_process_alive(&mut self) -> bool {
        match &self.auth_process {
            Some(auth_process) => {
                let mut auth_process = auth_process.lock().await;
                match auth_process.process.poll() {
                    Some(exit_status) => {
                        println!("[IpcAuthController] Auth process exited with status {exit_status:?}, respawning...");
                        false
                    }
                    None => true,
                }
            }
            None => false,
        }
    }

    fn spawn_auth_process(&mut self) -> anyhow::Result<Arc<Mutex<AuthProcess>>> {
        let (ipc_server, ipc_server_name): (
            IpcOneShotServer<(
                IpcSender<AuthenticatorMessage>,
                IpcSender<IpcSender<AuthenticatorMessage>>,
            )>,
            String,
        ) = IpcOneShotServer::new().unwrap();

        let process = crate::app::utils::subprocess::dup_current_exe(&[
            OsStr::new(""),
            OsStr::new("--launch-webview"),
            OsStr::new("--account"),
            OsStr::new("UNUSED"),
            OsStr::new("--ipc"),
            OsStr::new(&ipc_server_name.clone()),
        ])?;

        let connection = AuthenticatorConnection::accept(ipc_server)?;

        Ok(Arc::new(Mutex::new(AuthProcess {
            process,
            connection,
        })))
    }
}
