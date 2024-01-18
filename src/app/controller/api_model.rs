use std::sync::Arc;

use tokio::sync::RwLockReadGuard;

use crate::app::api_model::{GameMap, UserModel};

pub struct ApiModel {
    pub user: Arc<UserModel>,
}

impl ApiModel {
    pub fn new() -> Self {
        Self {
            user: Arc::new(UserModel::new()),
        }
    }

    pub async fn get_game_map_read(&self) -> RwLockReadGuard<'_, GameMap> {
        self.user.games.read().await
    }
}
