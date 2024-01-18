use anyhow::anyhow;
use arkhost_api::clients::{self, common::ApiResult};
use arkhost_api::consts;
use arkhost_api::models::{api_arkhost, api_passport};
use derivative::Derivative;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::api_model::{GameRef, UserModel};

#[derive(Debug)]
pub enum RetrieveLogSpec {
    Latest {},
    Former {},
}

pub type CommandResult<T> = ApiResult<T>;
pub type Responder<T> = oneshot::Sender<CommandResult<T>>;

#[derive(Derivative)]
#[derivative(Debug)]
#[allow(unused)]
pub enum Operation {
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
        resp: Responder<Arc<UserModel>>,
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
    PreformCaptcha {
        account: String,
        captcha_info: api_arkhost::CaptchaResultInfo,
        resp: Responder<()>,
    },
    ConnectGameEventSource {
        #[derivative(Debug = "ignore")]
        resp: Responder<clients::arkhost::SseStream<anyhow::Result<api_arkhost::GameSseEvent>>>,
    },
}

#[derive(Debug)]
pub struct Command {
    pub user: Arc<UserModel>,
    pub op: Operation,
}

pub struct Controller {
    pub auth_client: Arc<clients::id_server::AuthClient>,
    pub arkhost_client: Arc<clients::arkhost::Client>,
    pub eventsource_client: Arc<clients::arkhost::EventSourceClient>,
}

impl Controller {
    pub fn new(auth_client: clients::id_server::AuthClient) -> Self {
        Self {
            auth_client: Arc::new(auth_client.clone()),
            arkhost_client: Arc::new(clients::arkhost::Client::new(
                consts::arkhost::API_BASE_URL,
                auth_client.clone(),
            )),
            eventsource_client: Arc::new(clients::arkhost::EventSourceClient::new(
                consts::arkhost::API_BASE_URL,
                auth_client,
            )),
        }
    }

    pub async fn run(&mut self, mut recv: mpsc::Receiver<Command>, stop: CancellationToken) {
        tokio::select! {
            _ = async {
                while let Some(cmd) = recv.recv().await {
                    match cmd.op {
                        Operation::Login {
                            email,
                            password,
                            resp,
                        } => _ = resp.send(self.login(email, password).await),
                        Operation::Auth { resp } => _ = resp.send(self.auth().await),
                        Operation::Logout { resp } => _ = resp.send(self.logout().await),
                        Operation::RetrieveGames { resp } => {
                            _ = resp.send(self.retrieve_games(cmd.user).await)
                        }
                        Operation::RetrieveGameDetails { account, resp } => {
                            _ = resp.send(self.retrieve_game_details(cmd.user, account).await)
                        }
                        Operation::RetrieveLog {
                            account,
                            spec,
                            resp,
                        } => _ = resp.send(self.retrieve_log(cmd.user, account, spec).await),
                        Operation::StartGame {
                            account,
                            captcha_token,
                            resp,
                        } => _ = resp.send(self.start_game(account, captcha_token).await),
                        Operation::StopGame { account, resp } => {
                            _ = resp.send(self.stop_game(account).await)
                        }
                        Operation::RestartGame { account, resp } => {
                            _ = resp.send(self.restart_game(account).await)
                        }
                        Operation::UpdateGameSettings {
                            account,
                            config,
                            resp,
                        } => _ = resp.send(self.update_game_settings(account, config).await),
                        Operation::PreformCaptcha {
                            account,
                            captcha_info,
                            resp,
                        } => _ = resp.send(self.update_captcha_info(account, captcha_info).await),
                        Operation::ConnectGameEventSource { resp } => {
                            _ = resp.send(self.eventsource_client.connect_games_sse())
                        }
                    }
                }
            } => {},
            _ = stop.cancelled() => {}
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

    pub async fn retrieve_games(
        &self,
        user_model: Arc<UserModel>,
    ) -> CommandResult<Arc<UserModel>> {
        let games = self.arkhost_client.get_games().await?;
        user_model.handle_retrieve_games_result(games).await;
        Ok(user_model)
    }

    pub async fn retrieve_game_details(
        &self,
        user_model: Arc<UserModel>,
        account: String,
    ) -> CommandResult<GameRef> {
        let game_details = self.arkhost_client.get_game(&account).await?;
        let game_ref = user_model.get_game(&account).await?;
        game_ref.game.write().await.details = Some(game_details);
        Ok(user_model.get_game(&account).await?)
    }

    pub async fn retrieve_log(
        &self,
        user_model: Arc<UserModel>,
        account: String,
        spec: RetrieveLogSpec,
    ) -> CommandResult<GameRef> {
        let game_ref = user_model.get_game(&account).await?;
        let mut game = game_ref.game.write().await;

        match spec {
            RetrieveLogSpec::Latest {} => {
                let mut latest_logs = vec![];
                let mut latest_log_cursor_back = 0;
                while latest_log_cursor_back == 0
                    || (game.log_cursor_front != 0
                        && latest_log_cursor_back > game.log_cursor_front + 1)
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
                if game.log_cursor_front != 0 {
                    latest_logs_truncate_len = latest_logs
                        .iter()
                        .enumerate()
                        .find(|(_i, x)| x.id <= game.log_cursor_front)
                        .map_or(latest_logs_truncate_len, |(i, _x)| i);
                }
                latest_logs.truncate(latest_logs_truncate_len);
                latest_logs.append(&mut game.logs);
                game.logs = latest_logs;
            }
            RetrieveLogSpec::Former {} => {
                let mut new_logs = self
                    .arkhost_client
                    .get_logs(&account, game.log_cursor_back)
                    .await?;

                game.logs.append(&mut new_logs.logs);
            }
        }
        game.log_cursor_front = game.logs.first().map_or(0, |x| x.id);
        game.log_cursor_back = game.logs.last().map_or(0, |x| x.id);

        drop(game);
        Ok(user_model.get_game(&account).await?)
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

    pub async fn update_captcha_info(
        &self,
        account: String,
        captcha_info: api_arkhost::CaptchaResultInfo,
    ) -> CommandResult<()> {
        self.arkhost_client
            .update_captcha_info(&account, captcha_info)
            .await
    }
}
