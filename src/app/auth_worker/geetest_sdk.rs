use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{AuthContext, AuthError, AuthWorker, Command};

pub struct GeeTestSdkAuthWorker {}

#[async_trait]
impl AuthWorker for GeeTestSdkAuthWorker {
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
                }
            } => {},
            _ = stop.cancelled() => {}
        }
    }
}

impl GeeTestSdkAuthWorker {
    pub fn new() -> Self {
        Self {}
    }

    #[allow(unused_variables)]
    async fn exec_cmd(&mut self, cmd: Command) {
        match cmd {
            Command::ArkHostBackground { resp, action } => {
                _ = resp.send(Err(
                    AuthError::Launch(anyhow::anyhow!("not implemented")).into()
                ));
            }
            Command::ArkHostCaptcha { resp, action } => {
                _ = resp.send(Err(
                    AuthError::Launch(anyhow::anyhow!("not implemented")).into()
                ));
            }
            Command::GeeTest {
                resp,
                gt,
                challenge,
            } => {
                _ = resp.send(Err(
                    AuthError::Launch(anyhow::anyhow!("not implemented")).into()
                ));
            }
            Command::LaunchAuthenticator { resp } => {
                _ = resp.send(Ok(()));
            }
        }
    }
}
