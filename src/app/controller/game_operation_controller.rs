use crate::app::auth_worker::AuthContext;
use crate::app::ui::*;
use crate::app::utils::notification;
use crate::app::webview::auth::AuthResult;
use arkhost_api::clients::common::ResponseError;
use arkhost_api::models::api_arkhost;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::Mutex;

use anyhow::anyhow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use super::app_state_controller::AppStateController;
use super::sender::Sender;
use super::ApiOperation;
use super::AuthCommand;

pub struct GameOperationController {
    app_state_controller: Arc<AppStateController>,
    sender: Arc<Sender>,
    captcha_states: Mutex<HashMap<String, (ChallengeInfo, CaptchaState)>>,
}

impl GameOperationController {
    pub fn new(app_state_controller: Arc<AppStateController>, sender: Arc<Sender>) -> Self {
        Self {
            sender,
            app_state_controller,
            captcha_states: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start_game(&self, account: String) {
        let (resp1, rx1) = oneshot::channel();
        let (resp2, rx2) = oneshot::channel();
        let auth_methods = [
            (
                AuthCommand::ArkHostBackground {
                    resp: resp1,
                    action: "login".into(),
                },
                rx1,
            ),
            (
                AuthCommand::ArkHostCaptcha {
                    resp: resp2,
                    action: "login".into(),
                },
                rx2,
            ),
        ];

        let invoke_auth = |account: String| async move {
            let (tx_command, rx_command) = mpsc::channel(1);
            let stop = CancellationToken::new();
            let _guard = stop.clone().drop_guard();
            self.sender
                .tx_auth_worker
                .send(AuthContext { rx_command, stop })
                .await?;

            for (auth_command, mut rx) in auth_methods {
                match self
                    .try_start_game(account.clone(), auth_command, &tx_command, &mut rx)
                    .await
                {
                    Ok(_) => {
                        return anyhow::Ok(());
                    }
                    Err(e) => {
                        if e.downcast_ref::<ResponseError<()>>().is_some_and(|x| {
                            x.internal_status_code.is_some_and(|x| {
                                x == arkhost_api::consts::error_code::CAPTCHA_ERROR
                            })
                        }) {
                            println!("[Controller] captcha failed starting game {account}");
                        } else {
                            println!("[Controller] Unexpected error starting game {account}: {e}");
                            notification::toast(
                                &format!("{account} 启动托管时出现意外错误"),
                                None,
                                &format!("{e}"),
                                None,
                            );
                            anyhow::bail!("提交失败：意外错误 {}", e);
                        }
                    }
                }
            }
            notification::toast(
                &format!("{account} 启动托管失败"),
                Some("人机验证失败"),
                "请尝试重新启动托管并再次进行人机验证",
                None,
            );
            anyhow::bail!("提交失败：人机验证失败");
        };

        let result = invoke_auth(account.clone()).await;

        if result.is_err() {
            eprintln!("[Controller] all attempts to start game {account} failed");
        } else {
            notification::toast(&format!("{account} 已启动托管"), None, "", None);
        }
        self.app_state_controller
            .exec(|x| x.set_game_request_state(account.clone(), GameOperationRequestState::Idle));
    }

    pub async fn stop_game(&self, account: String) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(
                ApiOperation::StopGame {
                    account: account.clone(),
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(_) => {
                notification::toast(&format!("{account} 已停止托管"), None, "", None);
            }
            Err(e) => {
                eprintln!("[Controller] Error stopping game {e}");
                notification::toast(
                    &format!("{account} 停止托管时出现错误"),
                    None,
                    &format!("{e}"),
                    None,
                );
            }
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

        let invoke_auth = |gt| async move {
            let (tx_command, rx_command) = mpsc::channel(1);
            let (resp, rx) = oneshot::channel();
            let stop = CancellationToken::new();
            let _guard = stop.clone().drop_guard();
            self.sender
                .tx_auth_worker
                .send(AuthContext { rx_command, stop })
                .await?;
            tx_command
                .send(AuthCommand::GeeTest {
                    resp,
                    gt,
                    challenge,
                })
                .await?;
            rx.await?
        };

        let auth_result = invoke_auth(gt.clone()).await;

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
                notification::toast(
                    &format!("{account} 进行登录滑块验证失败"),
                    None,
                    &format!("{e}\n请尝试在验证超时后重新启动托管"),
                    None,
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
            .sender
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
            notification::toast(
                &format!("{account} 提交登录滑块验证结果失败"),
                None,
                &format!("{e}\n请尝试在验证超时后重新启动托管"),
                None,
            );
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
        tx_command: &mpsc::Sender<AuthCommand>,
        rx_auth_result: &mut oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<()> {
        tx_command.send(auth_command).await?;
        let auth_result = rx_auth_result.await??;
        let captcha_token = match auth_result {
            AuthResult::ArkHostCaptchaTokenReCaptcha { token, .. } => token,
            AuthResult::ArkHostCaptchaTokenGeeTest { token, .. } => token,
            _ => {
                return Err(anyhow!("unexpected auth result: {auth_result:?}"));
            }
        };

        let (resp, mut rx) = oneshot::channel();
        self.sender
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
