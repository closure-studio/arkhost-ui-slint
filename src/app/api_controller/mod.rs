use anyhow::anyhow;
use arkhost_api::clients::{
    self,
    common::ApiResult
};
use arkhost_api::consts;
use arkhost_api::models::{api_arkhost, api_passport};
use std::{collections::HashMap, mem, sync::Arc};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, RwLock};

#[derive(Error, Debug)]
pub enum CommandFailedError {
    #[error("requested game {account} not found in GameInfo repository")]
    GameNotFound { account: String },
}

#[derive(Debug)]
pub enum RetrieveLogSpec {
    Latest {},
    Former {},
}

pub type CommandResult<T> = ApiResult<T>;
pub type Responder<T> = oneshot::Sender<CommandResult<T>>;
pub type GameRef = Arc<InnerGameRef>;
pub type GameMap = RwLock<HashMap<String, GameRef>>;

#[derive(Debug)]
#[allow(unused)]
pub enum Command {
    Login {
        email: String,
        password: String,
        resp: Responder<api_passport::User>,
    },
    Auth {
        resp: Responder<api_passport::User>,
    },
    Logout {
        resp: Responder<()>,
    },
    RetrieveGames {
        resp: Responder<Arc<GameMap>>,
    },
    RetrieveGameDetails {
        account: String,
        resp: Responder<GameRef>,
    },
    RetrieveLog {
        account: String,
        spec: RetrieveLogSpec,
        resp: Responder<GameRef>,
    },
    TruncateLog {
        account: String,
        limit: u32,
        resp: Responder<GameRef>,
    },
    StartGame {
        account: String,
        captcha_token: String,
        resp: Responder<()>,
    },
    StopGame {
        account: String,
        resp: Responder<()>,
    },
    RestartGame {
        account: String,
        resp: Responder<()>,
    },
    UpdateGameSettings {
        account: String,
        config: api_arkhost::GameConfigFields,
        resp: Responder<()>,
    },
    Stop {},
}

#[derive(Debug)]
pub struct InnerGameRef {
    pub order: i32,
    pub info: RwLock<GameInfo>,
}

#[derive(Debug, Clone)]
pub struct GameInfo {
    /// 游戏在游戏列表中的顺序，以1开始
    pub info: api_arkhost::GameInfo,
    pub details: Option<api_arkhost::GameDetails>,
    pub logs: Vec<api_arkhost::LogEntry>,
    pub log_cursor_back: u64,
    pub log_cursor_front: u64,
}

impl GameInfo {
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

pub struct Controller {
    pub games: Arc<GameMap>,
    pub auth_client: Arc<clients::id_server::AuthClient>,
    pub arkhost_client: Arc<clients::arkhost::Client>,
}

impl Controller {
    pub fn new(auth_client: clients::id_server::AuthClient) -> Self {
        Self {
            games: Arc::new(RwLock::new(HashMap::new())),
            auth_client: Arc::new(auth_client.clone()),
            arkhost_client: Arc::new(clients::arkhost::Client::new(
                consts::arkhost::API_BASE_URL,
                auth_client,
            )),
        }
    }

    pub async fn run(&mut self, mut recv: mpsc::Receiver<Command>) {
        while let Some(cmd) = recv.recv().await {
            match cmd {
                Command::Login {
                    email,
                    password,
                    resp,
                } => _ = resp.send(self.login(email, password).await),
                Command::Auth { resp } => _ = resp.send(self.auth().await),
                Command::Logout { resp } => _ = resp.send(self.logout().await),
                Command::RetrieveGames { resp } => _ = resp.send(self.retrieve_games().await),
                Command::RetrieveGameDetails { account, resp } => {
                    _ = resp.send(self.retrieve_game_details(account).await)
                }
                Command::RetrieveLog {
                    account,
                    spec,
                    resp,
                } => _ = resp.send(self.retrieve_log(account, spec).await),
                Command::TruncateLog {
                    account,
                    limit,
                    resp,
                } => _ = resp.send(self.truncate_log(account, limit).await),
                Command::StartGame {
                    account,
                    captcha_token,
                    resp,
                } => _ = resp.send(self.start_game(account, captcha_token).await),
                Command::StopGame { account, resp } => _ = resp.send(self.stop_game(account).await),
                Command::RestartGame { account, resp } => {
                    _ = resp.send(self.restart_game(account).await)
                }
                Command::UpdateGameSettings {
                    account,
                    config,
                    resp,
                } => _ = resp.send(self.update_game_settings(account, config).await),
                Command::Stop {} => {
                    break;
                }
            }
        }
    }

    pub async fn login(
        &self,
        email: String,
        password: String,
    ) -> CommandResult<api_passport::User> {
        self.auth_client.login(email, password).await
    }

    pub async fn auth(&self) -> CommandResult<api_passport::User> {
        let user = self.auth_client.get_user_info().await?;
        if user.status == api_passport::UserStatus::Banned {
            return Err(anyhow!("用户已被封禁，请联系管理员"));
        }

        Ok(user)
    }

    pub async fn logout(&mut self) -> CommandResult<()> {
        self.auth_client.logout();
        Ok(())
    }

    pub async fn retrieve_games(&mut self) -> CommandResult<Arc<GameMap>> {
        let mut games = self.arkhost_client.get_games().await?;
        let current_game_map = self.games.read().await;
        let mut game_map = HashMap::<String, GameRef>::new();
        while let Some(game) = games.pop() {
            let account = game.status.account.clone();
            let mut game_info = GameInfo::new(game);
            if let Some(cur_game_ref) = current_game_map.get(&account) {
                let mut cur_game_info = cur_game_ref.info.write().await;
                game_info.logs.append(&mut cur_game_info.logs);
                game_info.log_cursor_back = cur_game_info.log_cursor_back;
                game_info.log_cursor_front = cur_game_info.log_cursor_front;
                game_info.details = mem::replace(&mut cur_game_info.details, None);
            }

            game_map.insert(
                account,
                Arc::new(InnerGameRef {
                    order: games.len().try_into().unwrap(),
                    info: RwLock::new(game_info),
                }),
            );
        }

        let new_games = Arc::new(GameMap::new(game_map));
        drop(current_game_map);
        self.games = new_games;
        Ok(self.games.clone())
    }

    pub async fn retrieve_game_details(&self, account: String) -> CommandResult<GameRef> {
        let game_details = self.arkhost_client.get_game(&account).await?;

        let games_lock = self.games.write().await;
        let game_ref = match games_lock.get(&account) {
            None => Err(CommandFailedError::GameNotFound { account }),
            Some(game) => Ok(game.clone()),
        }?;
        game_ref.info.write().await.details = Some(game_details);
        Ok(game_ref)
    }

    pub async fn retrieve_log(
        &self,
        account: String,
        spec: RetrieveLogSpec,
    ) -> CommandResult<GameRef> {
        let game_ref = self.get_game(&account).await?;
        let mut game_lock = game_ref.info.write().await;

        match spec {
            RetrieveLogSpec::Latest {} => {
                let mut latest_logs = vec![];
                let mut latest_log_cursor_back = 0;
                while latest_log_cursor_back == 0
                    || (game_lock.log_cursor_front != 0
                        && latest_log_cursor_back > game_lock.log_cursor_front + 1)
                {
                    let mut resp = self
                        .arkhost_client
                        .get_logs(&account, latest_log_cursor_back)
                        .await?;
                    latest_log_cursor_back =
                        resp.logs.last().map_or(latest_log_cursor_back, |x| x.id);
                    latest_logs.append(&mut resp.logs);
                    if !resp.has_more {
                        break;
                    }
                }

                let mut latest_logs_truncate_len = latest_logs.len();
                if game_lock.log_cursor_front != 0 {
                    latest_logs_truncate_len = latest_logs
                        .iter()
                        .enumerate()
                        .find(|(_i, x)| x.id <= game_lock.log_cursor_front)
                        .map_or(latest_logs_truncate_len, |(i, _x)| i);
                }
                latest_logs.truncate(latest_logs_truncate_len);
                latest_logs.append(&mut game_lock.logs);
                game_lock.logs = latest_logs;
            }
            RetrieveLogSpec::Former {} => {
                let mut new_logs = self
                    .arkhost_client
                    .get_logs(&account, game_lock.log_cursor_back)
                    .await?;

                game_lock.logs.append(&mut new_logs.logs);
            }
        }
        game_lock.log_cursor_front = game_lock.logs.first().map_or(0, |x| x.id);
        game_lock.log_cursor_back = game_lock.logs.last().map_or(0, |x| x.id);

        drop(game_lock);
        Ok(game_ref)
    }

    pub async fn truncate_log(&self, account: String, limit: u32) -> CommandResult<GameRef> {
        let game_ref = self.get_game(&account).await?;
        {
            let game_ref = game_ref.clone();
            let mut game_ref_write = game_ref.info.write().await;
            Controller::do_truncate_logs(&mut game_ref_write, limit);
        }
        Ok(game_ref)
    }

    pub async fn start_game(&self, account: String, captcha_token: String) -> CommandResult<()> {
        self.arkhost_client
            .login_game(&account, &captcha_token)
            .await
    }

    pub async fn stop_game(&self, account: String) -> CommandResult<()> {
        let mut config = api_arkhost::GameConfigFields::new();
        config.is_stopped = Some(true);

        self.arkhost_client
            .update_game_config(&account, config)
            .await
    }

    pub async fn restart_game(&self, _account: String) -> CommandResult<()> {
        todo!()
    }

    pub async fn update_game_settings(
        &self,
        account: String,
        config: api_arkhost::GameConfigFields,
    ) -> CommandResult<()> {
        self.arkhost_client
            .update_game_config(&account.clone(), config)
            .await
    }

    async fn get_game(&self, account: &String) -> Result<GameRef, CommandFailedError> {
        match self.games.read().await.get(account) {
            None => Err(CommandFailedError::GameNotFound {
                account: account.clone(),
            }),
            Some(game) => Ok(game.clone()),
        }
    }

    fn do_truncate_logs(game: &mut GameInfo, limit: u32) {
        game.logs.truncate(limit.try_into().unwrap());
        game.log_cursor_back = if let Some(log_entry) = game.logs.last() {
            log_entry.id
        } else {
            0
        }
    }
}
