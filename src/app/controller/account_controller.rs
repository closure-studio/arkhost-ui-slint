use crate::app::ui::*;
use tokio::sync::oneshot;

use std::sync::Arc;

use super::ApiCommand;

pub struct AccountController {}

impl AccountController {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn login(
        &self,
        parent: Arc<super::ControllerHub>,
        account: String,
        password: String,
    ) {
        let (resp, mut rx) = oneshot::channel();
        match parent
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
                parent
                    .get_app_state()
                    .set_login_state(LoginState::Logged, "登录成功".into());

                parent
                    .game_controller
                    .refresh_games(parent.clone(), super::RefreshLogsCondition::Never)
                    .await;
            }
            Err(e) => {
                parent
                    .get_app_state()
                    .set_login_state(LoginState::Errored, format!("{:?}", e).into());
            }
        }
    }

    pub async fn auth(&self, parent: Arc<super::ControllerHub>) {
        parent
            .get_app_state()
            .set_login_state(LoginState::LoggingIn, "自动登录中……".into());
        let (resp, mut rx) = oneshot::channel();
        match parent
            .send_api_request(ApiCommand::Auth { resp }, &mut rx)
            .await
        {
            Ok(user) => {
                println!(
                    "[Controller] Auth success: {} {}",
                    user.user_email, user.uuid
                );
                parent
                    .get_app_state()
                    .set_login_state(LoginState::Logged, "登录成功".into());

                parent
                    .game_controller
                    .refresh_games(
                        parent.clone(),
                        super::RefreshLogsCondition::OnLogsViewOpened,
                    )
                    .await;
            }
            Err(e) => {
                parent.get_app_state().set_login_state(
                    LoginState::Errored,
                    format!("自动登录失败，请重试或检查网络环境\n{:?}", e).into(),
                );
            }
        }
    }
}
