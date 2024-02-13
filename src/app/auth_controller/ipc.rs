use std::sync::Arc;

use super::AuthContext;
use super::AuthController;
use super::Command;
use crate::app::ipc::AuthenticatorServerSideChannel;
use crate::app::ipc::{AuthenticatorConnection, AuthenticatorMessage};
use crate::app::webview::auth::{AuthAction, AuthResult};
use anyhow::anyhow;
use async_trait::async_trait;
use ipc_channel::ipc::IpcOneShotServer;
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

impl IpcAuthController {
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
        }
    }
}

#[async_trait]
impl AuthController for IpcAuthController {
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
        in_background: bool,
    ) -> anyhow::Result<AuthResult> {
        let auth_process = self.ensure_auth_process().await?;
        let mut auth_process = auth_process.lock().await;
        auth_process
            .connection
            .send_command(AuthenticatorMessage::SetVisible {
                x: 250.,
                y: 250.,
                visible: !in_background,
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
            IpcOneShotServer<AuthenticatorServerSideChannel>,
            String,
        ) = IpcOneShotServer::new().unwrap();

        let mut process = crate::app::utils::subprocess::dup_current_exe(&[
            "",
            "--launch-webview",
            "--account",
            "UNUSED",
            "--ipc",
            &ipc_server_name,
        ])?;

        match AuthenticatorConnection::accept(ipc_server) {
            Ok(connection) => Ok(Arc::new(Mutex::new(AuthProcess {
                process,
                connection,
            }))),
            Err(e) => {
                _ = process.terminate();
                Err(e)
            }
        }
    }
}
