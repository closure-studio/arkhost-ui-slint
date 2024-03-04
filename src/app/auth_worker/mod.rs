#[cfg(feature = "desktop-app")]
pub mod ipc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::webview::auth::AuthResult;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("authentication failed: {0:?}")]
    Auth(AuthResult),
    #[error("authenticator WebView launch failed")]
    LaunchWebView,
    #[error("authenticator launch failed {0}")]
    Launch(anyhow::Error),
    #[error("authenticator process unexpectedly exited with exit status '{0}'")]
    ProcessExited(String),
}

pub struct AuthContext {
    pub rx_command: mpsc::Receiver<Command>,
    pub stop: CancellationToken,
}

pub enum Command {
    LaunchAuthenticator {
        resp: oneshot::Sender<Result<(), AuthError>>,
    },
    ArkHostBackground {
        resp: oneshot::Sender<anyhow::Result<AuthResult>>,
        action: String,
    },
    ArkHostCaptcha {
        resp: oneshot::Sender<anyhow::Result<AuthResult>>,
        action: String,
    },
    GeeTest {
        resp: oneshot::Sender<anyhow::Result<AuthResult>>,
        gt: String,
        challenge: String,
    },
}

#[async_trait]
pub trait AuthWorker {
    async fn run(&mut self, tx: mpsc::Receiver<AuthContext>, stop: CancellationToken);
}
