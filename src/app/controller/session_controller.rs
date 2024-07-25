use super::{
    api_user_model::ApiUserModel, app_state_controller::AppStateController,
    game_controller::GameController, ota_controller::OtaController, sender::Sender,
    slot_controller::SlotController, ApiOperation, ApiResult,
};
use async_scoped::TokioScope;
use log::{debug, warn};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tokio_util::sync::{CancellationToken, DropGuard};

pub struct SessionController {
    pub api_user_model: Arc<ApiUserModel>,
    pub app_state_controller: Arc<AppStateController>,
    pub sender: Arc<Sender>,
    pub game_controller: Arc<GameController>,
    pub slot_controller: Arc<SlotController>,
    pub ota_controller: Arc<OtaController>,

    pub stop_connections: Mutex<Option<DropGuard>>,
}

impl SessionController {
    pub fn new(
        api_user_model: Arc<ApiUserModel>,
        app_state_controller: Arc<AppStateController>,
        sender: Arc<Sender>,
        game_controller: Arc<GameController>,
        slot_controller: Arc<SlotController>,
        ota_controller: Arc<OtaController>,
    ) -> Self {
        Self {
            api_user_model,
            app_state_controller,
            sender,
            game_controller,
            slot_controller,
            ota_controller,
            stop_connections: Mutex::new(None),
        }
    }

    pub async fn authorize_with_account(&self, account: String, password: String) -> ApiResult<()> {
        let (resp, mut rx) = oneshot::channel();
        self.sender
            .send_api_request(
                ApiOperation::Login {
                    email: account,
                    password,
                    resp,
                },
                &mut rx,
            )
            .await
    }

    pub async fn authorize_with_stored_token(&self) -> ApiResult<()> {
        let (resp, mut rx) = oneshot::channel();
        self.sender
            .send_api_request(ApiOperation::Auth { resp }, &mut rx)
            .await
    }

    pub async fn create_user_model(&self) {
        self.api_user_model.user.clear().await;
        self.game_controller.try_ensure_resources().await;
        let _ = TokioScope::scope_and_block(|s| {
            s.spawn(self.fetch_site_config());
            s.spawn(self.slot_controller.refresh_slots());
            s.spawn(self.spawn_sse_event_loop());
            s.spawn(self.ota_controller.check_release_update());
        });
    }

    pub async fn on_post_create_user_model(&self) {
        self.ota_controller.try_auto_update_release().await;
    }

    pub async fn spawn_sse_event_loop(&self) {
        let stop_connection_token = CancellationToken::new();
        if self
            .stop_connections
            .lock()
            .unwrap()
            .replace(stop_connection_token.clone().drop_guard())
            .is_some()
        {
            debug!("terminated connections in previous session");
        }

        let game_controller = self.game_controller.clone();
        tokio::spawn(async move {
            if let Err(e) = game_controller
                .run_sse_event_loop(stop_connection_token.clone())
                .await
            {
                warn!("games SSE connection terminated with error: {e:?}");
            }
        });
    }

    async fn fetch_site_config(&self) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(ApiOperation::GetSiteConfig { resp }, &mut rx)
            .await
        {
            Ok(cfg) => self.app_state_controller.exec(move |x| {
                x.state_globals(move |s| {
                    if let Some(announcement) = cfg.announcement {
                        s.set_site_announcement(announcement.into());
                    }
                    s.set_is_site_under_maintenance(
                        cfg.is_under_maintenance || !cfg.allow_game_login,
                    );
                })
            }),
            Err(e) => {
                warn!("error fetching site config: {e}");
            }
        }
    }
}
