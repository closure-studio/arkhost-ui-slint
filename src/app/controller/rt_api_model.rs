use std::sync::Arc;

use tokio::sync::RwLockReadGuard;

use crate::app::rt_api_model::{GameMap, RtUserModel};

pub struct RtApiModel {
    pub user: Arc<RtUserModel>,
}

impl RtApiModel {
    pub fn new() -> Self {
        Self {
            user: Arc::new(RtUserModel::new()),
        }
    }

    pub async fn game_map_read(&self) -> RwLockReadGuard<'_, GameMap> {
        self.user.games.read().await
    }
}
