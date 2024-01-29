use crate::app::ui::*;
use tokio::sync::oneshot;
use tokio_util::sync::{CancellationToken, DropGuard};

use std::sync::{Arc, Mutex};

use super::{
    app_state_controller::AppStateController, game_controller::GameController, sender::Sender, rt_api_model::RtApiModel, slot_controller::SlotController, ApiOperation
};

pub struct SessionController {
    pub rt_api_model: Arc<RtApiModel>,
    pub app_state_controller: Arc<AppStateController>,
    pub sender: Arc<Sender>,
    pub game_controller: Arc<GameController>,
    pub slot_controller: Arc<SlotController>,
    pub stop_connections: Mutex<Option<DropGuard>>,
}

impl SessionController {
    pub fn new(
        rt_api_model: Arc<RtApiModel>,
        app_state_controller: Arc<AppStateController>,
        sender: Arc<Sender>,
        game_controller: Arc<GameController>,
        slot_controller: Arc<SlotController>
    ) -> Self {
        Self {
            rt_api_model,
            app_state_controller,
            sender,
            game_controller,
            slot_controller,
            stop_connections: Mutex::new(None),
        }
    }

    pub async fn login(&self, account: String, password: String) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(
                ApiOperation::Login {
                    email: account,
                    password,
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(()) => {
                println!(
                    "[Controller] Logged in with password authorization, running post-login callback...",
                );
                self.on_login().await;
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Logged, "登录成功".into()));
            }
            Err(e) => {
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Errored, format!("{:?}", e)));
            }
        }
    }

    pub async fn auth(&self) {
        self.app_state_controller
            .exec(|x| x.set_login_state(LoginState::LoggingIn, "自动登录中……".into()));
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(ApiOperation::Auth { resp }, &mut rx)
            .await
        {
            Ok(()) => {
                println!(
                    "[Controller] Logged in with token authorization, running post-login callback..."
                );
                self.on_login().await;
                self.app_state_controller
                    .exec(|x| x.set_login_state(LoginState::Logged, "登录成功".into()));
            }
            Err(e) => {
                self.app_state_controller.exec(|x| {
                    x.set_login_state(
                        LoginState::Errored,
                        format!("自动登录失败，请重试或检查网络环境\n{:?}", e),
                    )
                });
            }
        }
    }

    pub async fn start_sse_event_loop(&self) {
        let stop_connection_token = CancellationToken::new();
        if self
            .stop_connections
            .lock()
            .unwrap()
            .replace(stop_connection_token.clone().drop_guard())
            .is_some()
        {
            println!("[Controller] Terminated connections in previous session");
        }

        let game_controller = self.game_controller.clone();
        tokio::spawn(async move {
            if let Err(e) = game_controller
                .run_sse_event_loop(stop_connection_token.clone())
                .await
            {
                eprintln!("[Controller] Games SSE connection terminated with error: {e:?}");
            }
        });
    }

    async fn on_login(&self) {
        self.rt_api_model.user.clear().await;
        self.game_controller.try_ensure_resources().await;
        self.slot_controller.refresh_slots().await;
        self.start_sse_event_loop().await;
    }
}
