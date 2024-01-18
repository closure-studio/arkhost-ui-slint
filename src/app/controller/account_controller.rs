use crate::app::ui::*;
use tokio::sync::oneshot;
use tokio_util::sync::{CancellationToken, DropGuard};

use std::sync::{Arc, Mutex};

use super::{
    app_state_controller::AppStateController, game_controller::GameController,
    request_controller::RequestController, ApiOperation,
};

pub struct AccountController {
    pub app_state_controller: Arc<AppStateController>,
    pub request_controller: Arc<RequestController>,
    pub game_controller: Arc<GameController>,
    pub stop_connections: Mutex<Option<DropGuard>>,
}

impl AccountController {
    pub fn new(
        app_state_controller: Arc<AppStateController>,
        request_controller: Arc<RequestController>,
        game_controller: Arc<GameController>,
    ) -> Self {
        Self {
            app_state_controller,
            request_controller,
            game_controller,
            stop_connections: Mutex::new(None),
        }
    }

    pub async fn login(&self, account: String, password: String) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .request_controller
            .send_api_request(
                ApiOperation::Login {
                    email: account.into(),
                    password: password.into(),
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(user) => {
                println!(
                    "[Controller] Logged in with password authorization, running post-login callback... [{} {}]",
                    user.user_email, user.uuid
                );
                self.on_login().await;
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Logged, "登录成功".into()));
            }
            Err(e) => {
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Errored, format!("{:?}", e).into()));
            }
        }
    }

    pub async fn auth(&self) {
        self.app_state_controller
            .exec(|x| x.set_login_state(LoginState::LoggingIn, "自动登录中……".into()));
        let (resp, mut rx) = oneshot::channel();
        match self
            .request_controller
            .send_api_request(ApiOperation::Auth { resp }, &mut rx)
            .await
        {
            Ok(user) => {
                println!(
                    "[Controller] Logged in with token authorization, running post-login callback... [{} {}]",
                    user.user_email, user.uuid
                );
                self.on_login().await;
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Logged, "登录成功".into()));
            }
            Err(e) => {
                self.app_state_controller.exec(|x| {
                    x.set_login_state(
                        LoginState::Errored,
                        format!("自动登录失败，请重试或检查网络环境\n{:?}", e).into(),
                    )
                });
            }
        }
    }

    async fn on_login(&self) {
        let stop_connection_token = CancellationToken::new();
        if let Some(_) = self
            .stop_connections
            .lock()
            .unwrap()
            .replace(stop_connection_token.clone().drop_guard())
        {
            println!("[Controller] Terminated connections in previous session");
        }

        self.game_controller.try_ensure_resources().await;
        let game_controller = self.game_controller.clone();
        tokio::spawn(async move {
            if let Err(e) = game_controller
                .connect_games_sse(stop_connection_token)
                .await
            {
                eprintln!("[Controller] Games SSE connection terminated with error: {e:?}");
            }
        });
    }
}
