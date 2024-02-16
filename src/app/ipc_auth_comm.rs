use super::webview::auth;
use futures_util::StreamExt;
use ipc_channel::asynch::IpcStream;
use ipc_channel::ipc::{self, IpcOneShotServer, IpcReceiver, IpcSender};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthenticatorCommError {
    #[error("failed to launch authenticator WebView")]
    LaunchWebViewFailed,
    #[error("error send authenticator message")]
    SendError(anyhow::Error),
    #[error("error receiving authenticator message")]
    RecvError(anyhow::Error),
    #[error("expected response of type {expected}, got: {got}")]
    InvalidResponse { expected: String, got: String },
    #[error("authenticator was unexpectedly closed")]
    AuthenticatorClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthenticatorMessage {
    Acknowledged,
    SetVisible { x: f32, y: f32, visible: bool },
    PerformAction { action: auth::AuthAction },
    Result { result: auth::AuthResult },
    CloseRequested,
    ReloadRequested,
    Closed,
    LaunchWebViewFailed,
}

pub struct AuthenticatorConnection {
    /// 发送消息给当前客户端（WebView连接父进程IPC服务器，属于客户端）的Sender, 目前只实现单客户端
    pub tx_client_sender: IpcSender<AuthenticatorMessage>,
    pub rx_client_receiver: IpcStream<AuthenticatorMessage>,
}

pub type AuthenticatorServerSideChannel = (
    IpcSender<AuthenticatorMessage>,
    IpcSender<IpcSender<AuthenticatorMessage>>,
);

impl AuthenticatorConnection {
    /// 使用例：
    /// ```
    /// use ipc_channel::ipc::{self, IpcOneShotServer, IpcReceiver, IpcSender};
    ///
    /// let (ipc_server, ipc_server_name): (
    ///     IpcOneShotServer<AuthenticatorServerSideChannel>,
    ///     String,
    /// ) = IpcOneShotServer::new()
    /// // ...
    /// let connection = AuthenticatorConnection::accept(ipc_server)?;
    /// ```
    pub fn accept(
        ipc_server: IpcOneShotServer<AuthenticatorServerSideChannel>,
    ) -> anyhow::Result<AuthenticatorConnection> {
        let (_, (tx_client_sender, rx_client_sender_sender)) = ipc_server.accept()?;
        let (rx_client_sender, rx_client_receiver): (
            IpcSender<AuthenticatorMessage>,
            IpcReceiver<AuthenticatorMessage>,
        ) = ipc::channel()?;
        rx_client_sender_sender.send(rx_client_sender)?;
        println!("[AuthenticatorConnection] Accepted Authenticator connection");
        Ok(Self {
            tx_client_sender,
            rx_client_receiver: rx_client_receiver.to_stream(),
        })
    }

    pub async fn send_command(
        &mut self,
        command: AuthenticatorMessage,
    ) -> Result<(), AuthenticatorCommError> {
        self.tx_client_sender.send(command).map_err(|e| AuthenticatorCommError::SendError(e.into()))?;
        let event = self.rx_client_receiver.next().await;
        match event {
            Some(Err(e)) => Err(AuthenticatorCommError::RecvError(e.into()).into()),
            Some(Ok(AuthenticatorMessage::Acknowledged)) => Ok(()),
            Some(Ok(AuthenticatorMessage::LaunchWebViewFailed)) => {
                Err(AuthenticatorCommError::LaunchWebViewFailed)
            }
            _ => Err(AuthenticatorCommError::InvalidResponse {
                expected: "Acknowledged".into(),
                got: format!("{event:?}"),
            }
            .into()),
        }
    }

    pub async fn send_auth_action(
        &mut self,
        action: AuthenticatorMessage,
    ) -> Result<auth::AuthResult, AuthenticatorCommError> {
        self.send_command(action).await?;

        let event = self.rx_client_receiver.next().await;
        match event {
            Some(Err(e)) => Err(AuthenticatorCommError::RecvError(e.into()).into()),
            Some(Ok(AuthenticatorMessage::Result { result })) => Ok(result),
            Some(Ok(AuthenticatorMessage::Closed)) => {
                Err(AuthenticatorCommError::AuthenticatorClosed.into())
            }
            _ => Err(AuthenticatorCommError::InvalidResponse {
                expected: "Result".into(),
                got: format!("{event:?}"),
            }
            .into()),
        }
    }

    pub fn close(&self) {
        _ = self
            .tx_client_sender
            .send(AuthenticatorMessage::CloseRequested);
    }
}
