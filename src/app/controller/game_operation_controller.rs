use crate::app::ui::*;
use crate::app::webview::auth::AuthResult;
use arkhost_api::models::api_arkhost;
use tokio::sync::oneshot;
use tokio::sync::Mutex;

use anyhow::anyhow;
use std::collections::HashMap;
use std::sync::Arc;

use super::app_state_controller::AppStateController;
use super::request_controller::RequestController;
use super::ApiOperation;
use super::AuthCommand;

pub struct GameOperationController {
    app_state_controller: Arc<AppStateController>,
    request_controller: Arc<RequestController>,
    captcha_states: Mutex<HashMap<String, (ChallengeInfo, CaptchaState)>>,
}

impl GameOperationController {
    pub fn new(
        app_state_controller: Arc<AppStateController>,
        request_controller: Arc<RequestController>,
    ) -> Self {
        Self {
            request_controller,
            app_state_controller,
            captcha_states: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start_game(&self, account: String) {
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
                .try_start_game(account.clone(), auth_command, &mut rx)
                .await
            {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => println!("[Controller] failed attempting to start game {account}: {e}"),
            }
        }
        _ = self
            .request_controller
            .tx_auth_controller
            .send(AuthCommand::HideWindow {})
            .await;

        if !success {
            eprintln!("[Controller] all attempts to start game {account} failed");
        }
        self.app_state_controller
            .exec(|x| x.set_game_request_state(account.clone(), GameOperationRequestState::Idle));
    }

    pub async fn stop_game(&self, account: String) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .request_controller
            .send_api_request(
                ApiOperation::StopGame {
                    account: account.clone(),
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(_) => {}
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        self.app_state_controller
            .exec(|x| x.set_game_request_state(account.clone(), GameOperationRequestState::Idle));
    }

    pub async fn try_preform_game_captcha(
        &self,
        account: String,
        gt: String,
        challenge: String,
    ) -> anyhow::Result<()> {
        match self.captcha_states.lock().await.get(&account) {
            Some((
                ChallengeInfo {
                    gt: existing_gt,
                    challenge: existing_challenge,
                },
                _,
            )) if existing_gt == &gt && existing_challenge == &challenge => {
                return Err(anyhow!("challenge is still running or used"));
            }
            _ => {}
        }
        let challenge_info = ChallengeInfo {
            gt: gt.clone(),
            challenge: challenge.clone(),
        };

        self.captcha_states.lock().await.insert(
            account.clone(),
            (challenge_info.clone(), CaptchaState::Running),
        );

        let (resp, mut rx) = oneshot::channel();
        let auth_result = self
            .request_controller
            .send_auth_request(
                AuthCommand::AuthGeeTest {
                    resp,
                    gt: gt.clone(),
                    challenge,
                },
                &mut rx,
            )
            .await;
        _ = self
            .request_controller
            .tx_auth_controller
            .send(AuthCommand::HideWindow {})
            .await;
        let captcha_info = match auth_result.and_then(|result| match result {
            AuthResult::GeeTestAuth { token, .. } => {
                serde_json::de::from_str::<api_arkhost::CaptchaResultInfo>(&token)
                    .map_err(anyhow::Error::from)
            }
            _ => anyhow::Result::Err(anyhow!("unexpected auth result: {result:?}")),
        }) {
            Ok(captcha_info) => captcha_info,
            Err(e) => {
                eprintln!(
                    "[Controller] Error performing game captcha (invoking authenticator) {e}"
                );
                self.captcha_states
                    .lock()
                    .await
                    .insert(account.clone(), (challenge_info, CaptchaState::Failed));
                return Err(e);
            }
        };

        let (resp, mut rx) = oneshot::channel();
        if let Err(e) = self
            .request_controller
            .send_api_request(
                ApiOperation::PreformCaptcha {
                    account: account.clone(),
                    captcha_info,
                    resp,
                },
                &mut rx,
            )
            .await
        {
            eprintln!("[Controller] Error performing game captcha (updating game config) {e}");
            self.captcha_states
                .lock()
                .await
                .insert(account.clone(), (challenge_info, CaptchaState::Failed));
            return Err(e);
        }

        self.captcha_states
            .lock()
            .await
            .insert(account.clone(), (challenge_info, CaptchaState::Succeeded));
        Ok(())
    }

    async fn try_start_game(
        &self,
        account: String,
        auth_command: AuthCommand,
        rx_auth_controller: &mut oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<()> {
        let auth_result = self
            .request_controller
            .send_auth_request(auth_command, rx_auth_controller)
            .await?;
        let captcha_token = match auth_result {
            AuthResult::ArkHostCaptchaTokenReCaptcha { token, .. } => token,
            AuthResult::ArkHostCaptchaTokenGeeTest { token, .. } => token,
            _ => {
                return Err(anyhow!("unexpected auth result: {auth_result:?}"));
            }
        };

        let (resp, mut rx) = oneshot::channel();
        self.request_controller
            .send_api_request(
                ApiOperation::StartGame {
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

enum CaptchaState {
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone)]
struct ChallengeInfo {
    gt: String,
    challenge: String,
}
