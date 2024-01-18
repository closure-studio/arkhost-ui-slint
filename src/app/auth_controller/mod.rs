#[cfg(feature = "desktop-app")]
pub mod ipc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::webview::auth::AuthResult;

pub enum Command {
    AuthArkHostBackground { resp: oneshot::Sender<anyhow::Result<AuthResult>>, action: String },
    AuthArkHostCaptcha { resp: oneshot::Sender<anyhow::Result<AuthResult>>, action: String },
    AuthGeeTest { resp: oneshot::Sender<anyhow::Result<AuthResult>>, gt: String, challenge: String },
    HideWindow { },
}

#[async_trait]
pub trait AuthController {
    async fn run(&mut self, tx: mpsc::Receiver<Command>, stop: CancellationToken);
}
