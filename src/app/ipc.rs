use super::webview::auth;
use futures_util::StreamExt;
use ipc_channel::asynch::IpcStream;
use ipc_channel::ipc::{self, IpcOneShotServer, IpcReceiver, IpcSender};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthenticatorConnectionError {
    #[error("error receiving authenticator message")]
    RecvError(#[from] anyhow::Error),
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
}

pub struct AuthenticatorConnection {
    /// 发送消息给当前客户端的Sender, 目前只实现单客户端
    pub tx_client_sender: IpcSender<AuthenticatorMessage>,
    pub rx_client_receiver: IpcStream<AuthenticatorMessage>,
}

impl AuthenticatorConnection {
    /// 使用例：
    /// ```
    /// use ipc_channel::ipc::{self, IpcOneShotServer, IpcReceiver, IpcSender};
    /// 
    /// let (ipc_server, ipc_server_name): (
    ///     IpcOneShotServer<(
    ///         IpcSender<AuthenticatorMessage>,
    ///         IpcSender<IpcSender<AuthenticatorMessage>>,
    ///     )>,
    ///     String,
    /// ) = IpcOneShotServer::new()
    /// // ...
    /// let connection = AuthenticatorConnection::accept(ipc_server)?;
    /// ```
    pub fn accept(
        ipc_server: IpcOneShotServer<(
            IpcSender<AuthenticatorMessage>,
            IpcSender<IpcSender<AuthenticatorMessage>>,
        )>,
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

    pub async fn send_command(&mut self, command: AuthenticatorMessage) -> anyhow::Result<()> {
        self.tx_client_sender.send(command)?;
        let event = self.rx_client_receiver.next().await;
        match event {
            Some(Err(e)) => Err(AuthenticatorConnectionError::RecvError(e.into()).into()),
            Some(Ok(AuthenticatorMessage::Acknowledged)) => Ok(()),
            _ => Err(AuthenticatorConnectionError::InvalidResponse {
                expected: "Acknowledged".into(),
                got: format!("{:?}", event),
            }
            .into()),
        }
    }

    pub async fn send_auth_action(
        &mut self,
        action: AuthenticatorMessage,
    ) -> anyhow::Result<auth::AuthResult> {
        self.send_command(action).await?;

        let event = self.rx_client_receiver.next().await;
        match event {
            Some(Err(e)) => Err(AuthenticatorConnectionError::RecvError(e.into()).into()),
            Some(Ok(AuthenticatorMessage::Result { result })) => Ok(result),
            Some(Ok(AuthenticatorMessage::Closed)) => {
                Err(AuthenticatorConnectionError::AuthenticatorClosed.into())
            }
            _ => Err(AuthenticatorConnectionError::InvalidResponse {
                expected: "Result".into(),
                got: format!("{:?}", event),
            }
            .into()),
        }
    }

    pub fn close(&self) {
        _ = self.tx_client_sender.send(AuthenticatorMessage::CloseRequested);
    }
}
