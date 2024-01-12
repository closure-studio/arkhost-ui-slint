use crate::app::ui::*;
use tokio::sync::oneshot;

use std::sync::Arc;

use super::{
    app_state_controller::AppStateController, game_controller::GameController,
    request_controller::RequestController, ApiCommand,
};

pub struct AccountController {
    pub request_controller: Arc<RequestController>,
    pub app_state_controller: Arc<AppStateController>,
    pub game_controller: Arc<GameController>,
}

impl AccountController {
    pub async fn login(&self, account: String, password: String) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .request_controller
            .send_api_request(
                ApiCommand::Login {
                    email: account.into(),
                    password: password.into(),
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(user) => {
                println!("[Controller] Logged in: {} {}", user.user_email, user.uuid);
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Logged, "登录成功".into()));
                self.game_controller
                    .refresh_games(super::RefreshLogsCondition::Never)
                    .await;
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
            .send_api_request(ApiCommand::Auth { resp }, &mut rx)
            .await
        {
            Ok(user) => {
                println!(
                    "[Controller] Auth success: {} {}",
                    user.user_email, user.uuid
                );
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Logged, "登录成功".into()));

                self.game_controller
                    .refresh_games(super::RefreshLogsCondition::OnLogsViewOpened)
                    .await;
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
}
