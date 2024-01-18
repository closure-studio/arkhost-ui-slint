use super::api_model::ApiModel;
use super::{ApiOperation, ApiCommand, AuthCommand, AssetCommand, ApiResult, AuthResult, AssetResult, ApiError};
use tokio::sync::oneshot;
use tokio::sync::mpsc;
use std::fmt::Debug;
use std::sync::Arc;

pub struct RequestController {
    pub api_model: Arc<ApiModel>,
    pub tx_api_controller: mpsc::Sender<ApiCommand>,
    pub tx_auth_controller: mpsc::Sender<AuthCommand>,
    pub tx_asset_controller: mpsc::Sender<AssetCommand>,
}

impl RequestController {
    pub async fn send_api_command(&self, op: ApiOperation) -> ApiResult<()> {
        self.tx_api_controller
            .send(ApiCommand { user: self.api_model.user.clone(), op })
            .await
            .map_err(ApiError::CommandSendError::<ApiCommand>)?;

        Ok(())
    }

    pub async fn send_api_request<T>(
        &self,
        op: ApiOperation,
        rx: &mut oneshot::Receiver<ApiResult<T>>,
    ) -> ApiResult<T>
    where
        T: 'static + Send + Sync + Debug,
    {
        self.send_api_command(op).await?;
        let recv = rx.await.map_err(ApiError::<T>::RespRecvError)?;
        recv
    }

    pub async fn send_auth_request(
        &self,
        command: AuthCommand,
        rx: &mut oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<AuthResult> {
        self.tx_auth_controller.send(command).await?;
        let auth_res = rx.await?;
        Ok(auth_res?)
    }

    pub async fn send_asset_command(&self, command: AssetCommand) -> AssetResult<()> {
        self.tx_asset_controller
            .send(command)
            .await
            .map_err(ApiError::CommandSendError::<AssetCommand>)?;

        Ok(())
    }

    pub async fn send_asset_request<T>(
        &self,
        command: AssetCommand,
        rx: &mut oneshot::Receiver<AssetResult<T>>,
    ) -> AssetResult<T>
    where
        T: 'static + Send + Sync + Debug,
    {
        self.send_asset_command(command).await?;
        let recv = rx.await.map_err(ApiError::<T>::RespRecvError)?;
        recv
    }
}