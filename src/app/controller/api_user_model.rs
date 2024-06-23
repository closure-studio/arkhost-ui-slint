use std::sync::Arc;

use tokio::sync::RwLockReadGuard;

use crate::app::api_user_model::{ApiUser, GameMap, SlotMap};

pub struct ApiUserModel {
    pub user: Arc<ApiUser>,
}

impl ApiUserModel {
    pub fn new() -> Self {
        Self {
            user: Arc::new(ApiUser::new()),
        }
    }

    pub async fn game_map_read(&self) -> RwLockReadGuard<'_, GameMap> {
        self.user.games.read().await
    }

    pub async fn slot_map_read(&self) -> RwLockReadGuard<'_, SlotMap> {
        self.user.slots.read().await
    }
}
