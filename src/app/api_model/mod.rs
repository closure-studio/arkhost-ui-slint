use anyhow::anyhow;
use arkhost_api::models::api_arkhost;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type GameRef = Arc<InnerGameRef>;
pub type GameMap = HashMap<String, GameRef>;
pub type GameMapSync = RwLock<GameMap>;

#[derive(Debug)]
pub struct InnerGameRef {
    pub order: AtomicI32,
    pub game: RwLock<GameEntry>,
}

#[derive(Debug, Clone)]
pub struct GameEntry {
    /// 游戏在游戏列表中的顺序，以1开始
    pub info: api_arkhost::GameInfo,
    pub details: Option<api_arkhost::GameDetails>,
    pub logs: Vec<api_arkhost::LogEntry>,
    pub log_cursor_back: u64,
    pub log_cursor_front: u64,
}

impl GameEntry {
    pub fn new(info: api_arkhost::GameInfo) -> Self {
        Self {
            info,
            details: None,
            logs: Vec::new(),
            log_cursor_back: 0,
            log_cursor_front: 0,
        }
    }
}

#[derive(Debug)]
pub struct UserModel {
    pub games: GameMapSync,
}

impl UserModel {
    pub fn new() -> Self {
        Self {
            games: RwLock::new(HashMap::new()),
        }
    }

    pub async fn handle_retrieve_games_result(&self, games_resp: Vec<api_arkhost::GameInfo>) {
        let mut game_map = self.games.write().await;
        
        let games_len = games_resp.len();
        for (i, game_info) in games_resp.into_iter().enumerate() {
            let account = game_info.status.account.clone();
            if let Some(game_ref) = game_map.get(&account) {
                let mut cur_game = game_ref.game.write().await;
                cur_game.info = game_info;
                game_ref.order.store(i as i32, Ordering::Release);
            } else {
                game_map.retain(|_, v| v.order.load(Ordering::Acquire) != i as i32);
                game_map.insert(
                    account,
                    Arc::new(InnerGameRef {
                        order: (i as i32).into(),
                        game: RwLock::new(GameEntry::new(game_info)),
                    }),
                );
            }
        }
        game_map.retain(|_, v| v.order.load(Ordering::Acquire) < games_len as i32);
    }

    pub async fn get_game(&self, account: &str) -> anyhow::Result<GameRef> {
        match self.games.read().await.get(account) {
            None => Err(anyhow!(format!("game with account '{}' not found", account))),
            Some(game) => Ok(game.clone()),
        }
    }
}
