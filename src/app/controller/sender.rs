use crate::app::auth_worker::AuthContext;

use super::api_user_model::ApiUserModel;
use super::{ApiCommand, ApiOperation, ApiResult, ApiWorkerError, AssetCommand, AssetResult};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub struct Sender {
    pub api_user_model: Arc<ApiUserModel>,
    pub tx_api_worker: mpsc::Sender<ApiCommand>,
    pub tx_auth_worker: mpsc::Sender<AuthContext>,
    pub tx_asset_worker: mpsc::Sender<AssetCommand>,
}

impl Sender {
    pub async fn send_api_command(&self, op: ApiOperation) -> ApiResult<()> {
        self.tx_api_worker
            .send(ApiCommand {
                user: self.api_user_model.user.clone(),
                op,
            })
            .await
            .map_err(ApiWorkerError::CommandSendError::<ApiCommand>)?;

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

        rx.await.map_err(ApiWorkerError::<T>::RespRecvError)?
    }

    pub async fn send_asset_command(&self, command: AssetCommand) -> AssetResult<()> {
        self.tx_asset_worker
            .send(command)
            .await
            .map_err(ApiWorkerError::CommandSendError::<AssetCommand>)?;

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

        rx.await.map_err(ApiWorkerError::<T>::RespRecvError)?
    }
}
