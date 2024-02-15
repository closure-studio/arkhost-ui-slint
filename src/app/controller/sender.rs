use crate::app::auth_worker::AuthContext;

use super::rt_api_model::RtApiModel;
use super::{
    ApiCommand, ApiError, ApiOperation, ApiResult, AssetCommand, AssetResult,
};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub struct Sender {
    pub rt_api_model: Arc<RtApiModel>,
    pub tx_api_worker: mpsc::Sender<ApiCommand>,
    pub tx_auth_worker: mpsc::Sender<AuthContext>,
    pub tx_asset_worker: mpsc::Sender<AssetCommand>,
}

impl Sender {
    pub async fn send_api_command(&self, op: ApiOperation) -> ApiResult<()> {
        self.tx_api_worker
            .send(ApiCommand {
                user: self.rt_api_model.user.clone(),
                op,
            })
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

        rx.await.map_err(ApiError::<T>::RespRecvError)?
    }

    pub async fn send_asset_command(&self, command: AssetCommand) -> AssetResult<()> {
        self.tx_asset_worker
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

        rx.await.map_err(ApiError::<T>::RespRecvError)?
    }
}
