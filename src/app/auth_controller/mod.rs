#[cfg(feature = "desktop-app")]
pub mod ipc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::webview::auth::AuthResult;

pub struct AuthContext {
    pub rx_command: mpsc::Receiver<Command>,
    pub stop: CancellationToken
}

pub enum Command {
    AuthArkHostBackground { resp: oneshot::Sender<anyhow::Result<AuthResult>>, action: String },
    AuthArkHostCaptcha { resp: oneshot::Sender<anyhow::Result<AuthResult>>, action: String },
    AuthGeeTest { resp: oneshot::Sender<anyhow::Result<AuthResult>>, gt: String, challenge: String },
}

#[async_trait]
pub trait AuthController {
    async fn run(&mut self, tx: mpsc::Receiver<AuthContext>, stop: CancellationToken);
}
