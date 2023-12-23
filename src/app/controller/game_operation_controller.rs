use crate::app::ui::*;
use crate::app::webview::auth::AuthResult;
use tokio::sync::oneshot;

use anyhow::anyhow;
use std::sync::Arc;

use super::ApiCommand;
use super::AuthCommand;

pub struct GameOperationController {}

impl GameOperationController {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn start_game(&self, parent: Arc<super::ControllerHub>, account: String) {
        let mut success = false;
        let (resp1, rx1) = oneshot::channel();
        let (resp2, rx2) = oneshot::channel();
        let auth_methods = vec![
            (
                AuthCommand::AuthArkHostBackground {
                    resp: resp1,
                    action: "login".into(),
                },
                rx1,
            ),
            (
                AuthCommand::AuthArkHostCaptcha {
                    resp: resp2,
                    action: "login".into(),
                },
                rx2,
            ),
        ];

        for (auth_command, mut rx) in auth_methods {
            match self
                .try_start_game(parent.clone(), account.clone(), auth_command, &mut rx)
                .await
            {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => println!("[Controller] failed attempting to start game {account}: {e}"),
            }
        }
        _ = parent
            .tx_auth_controller
            .send(AuthCommand::HideWindow {})
            .await;

        if !success {
            eprintln!("[Controller] all attempts to start game {account} failed");
        }
        parent
            .get_app_state()
            .set_game_request_state(account.clone(), GameOperationRequestState::Idle);
        parent
            .game_controller
            .refresh_games(parent.clone(), super::RefreshLogsCondition::Never)
            .await;
    }

    pub async fn stop_game(&self, parent: Arc<super::ControllerHub>, account: String) {
        let (resp, mut rx) = oneshot::channel();
        match parent
            .send_api_request(
                ApiCommand::StopGame {
                    account: account.clone(),
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(_) => {
                parent
                    .game_controller
                    .refresh_games(parent.clone(), super::RefreshLogsCondition::Never)
                    .await
            }
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        parent
            .get_app_state()
            .set_game_request_state(account.clone(), GameOperationRequestState::Idle);
    }

    async fn try_start_game(
        &self,
        parent: Arc<super::ControllerHub>,
        account: String,
        auth_command: AuthCommand,
        rx_auth_controller: &mut oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<()> {
        let auth_result = parent
            .send_auth_request(auth_command, rx_auth_controller)
            .await?;
        let captcha_token = match auth_result {
            AuthResult::ArkHostCaptchaTokenReCaptcha { token, .. } => token,
            AuthResult::ArkHostCaptchaTokenGeeTest { token, .. } => token,
            _ => {
                return Err(anyhow!("unexpected auth result: {auth_result:?}").into());
            }
        };

        let (resp, mut rx) = oneshot::channel();
        parent
            .send_api_request(
                ApiCommand::StartGame {
                    account,
                    captcha_token,
                    resp,
                },
                &mut rx,
            )
            .await
            .map(|_| ())
    }
}
