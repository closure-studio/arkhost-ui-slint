use super::webview::auth;
use futures_util::StreamExt;
use ipc_channel::asynch::IpcStream;
use ipc_channel::ipc::{IpcOneShotServer, IpcSender};
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
    pub tx_client: IpcSender<AuthenticatorMessage>,
    pub rx_client: IpcStream<AuthenticatorMessage>,
}

impl AuthenticatorConnection {
    /// 使用例：
    /// ```
    /// let (tx_server, tx_name): (IpcOneShotServer<IpcSender<AuthenticatorEvent>>, String) = IpcOneShotServer::new()?;
    /// let (rx_server, rx_name): (IpcOneShotServer<AuthenticatorEvent>, String) = IpcOneShotServer::new()?;
    /// // ...
    /// let conn = AuthenticatorConnection::accept(tx_server, rx_server);
    /// ```
    pub fn accept(
        tx_server: IpcOneShotServer<IpcSender<AuthenticatorMessage>>,
        rx_server: IpcOneShotServer<AuthenticatorMessage>,
    ) -> anyhow::Result<AuthenticatorConnection> {
        let (_, tx_client) = tx_server.accept()?;
        let (rx_client, _init_msg) = rx_server.accept()?;
        println!("[AuthenticatorConnection] Accepted Authenticator connection");
        Ok(Self {
            tx_client,
            rx_client: rx_client.to_stream(),
        })
    }

    pub async fn send_command(&mut self, command: AuthenticatorMessage) -> anyhow::Result<()> {
        self.tx_client.send(command)?;
        let event = self.rx_client.next().await;
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

        let event = self.rx_client.next().await;
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
        _ = self.tx_client.send(AuthenticatorMessage::CloseRequested);
    }
}
