pub mod api_user_model;
pub mod app_state_controller;
pub mod config_controller;
pub mod game_controller;
pub mod game_operation_controller;
pub mod image_controller;
pub mod ota_controller;
pub mod sender;
pub mod session_controller;
pub mod slot_controller;
pub mod user_controller;
extern crate alloc;

use self::api_user_model::ApiUserModel;
use self::config_controller::ConfigController;
use self::game_controller::GameController;
use self::game_operation_controller::GameOperationController;
use self::image_controller::ImageController;
use self::ota_controller::OtaController;
use self::sender::Sender;
use self::slot_controller::SlotController;
use self::user_controller::UserController;
use self::{app_state_controller::AppStateController, session_controller::SessionController};
use super::app_state::mapping::{GameOptionsMapping, SlotUpdateDraftMapping};
use super::app_state::AppState;
use super::auth_worker::AuthContext;
use super::ui::*;
use super::utils::ext_link;
use arkhost_api::models::api_quota::user_tier_availability_rank;
use slint::{Model, SharedString};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

type ApiOperation = super::api_worker::Operation;
type ApiCommand = super::api_worker::Command;
type AuthCommand = super::auth_worker::Command;
type AssetCommand = super::asset_worker::Command;

type ApiResult<T> = arkhost_api::clients::common::ApiResult<T>;
type AuthResult = super::auth::AuthResult;
type AssetResult<T> = anyhow::Result<T>;

#[derive(Debug, Clone)]
pub enum RefreshLogsCondition {
    #[allow(unused)]
    Always,
    OnLogsViewOpened,
    Never,
}

#[derive(Error, Debug)]
pub enum ApiError<T>
where
    T: 'static + Send + Sync + Debug,
{
    #[error("- 客户端内部错误 CommandSendError\n{0}")]
    CommandSendError(#[from] mpsc::error::SendError<T>),
    #[error("- 客户端内部错误 RespRecvError\n{0}")]
    RespRecvError(#[from] oneshot::error::RecvError),
}

pub struct UIContext {
    pub api_user_model: Arc<ApiUserModel>,
    pub app_state: Arc<Mutex<AppState>>,
    pub app_state_controller: Arc<AppStateController>,
    pub config_controller: Arc<ConfigController>,
    pub image_controller: Arc<ImageController>,
    pub session_controller: Arc<SessionController>,
    pub game_controller: Arc<GameController>,
    pub slot_controller: Arc<SlotController>,
    pub game_operation_controller: Arc<GameOperationController>,
    pub user_controller: Arc<UserController>,
    pub ota_controller: Arc<OtaController>,
}

pub struct UIMainThreadContext {
    pub refresh_game_timer: slint::Timer,
}

impl UIContext {
    pub fn new(
        app_state: AppState,
        api_user_model: Arc<ApiUserModel>,
        tx_api_worker: mpsc::Sender<ApiCommand>,
        tx_auth_worker: mpsc::Sender<AuthContext>,
        tx_asset_worker: mpsc::Sender<AssetCommand>,
    ) -> Self {
        let app_state = Arc::new(Mutex::new(app_state));
        let app_state_controller = Arc::new(AppStateController {
            app_state: app_state.clone(),
        });
        let config_controller = Arc::new(ConfigController::new(app_state_controller.clone()));
        let sender = Arc::new(Sender {
            api_user_model: api_user_model.clone(),
            tx_api_worker,
            tx_auth_worker,
            tx_asset_worker,
        });
        let image_controller = Arc::new(ImageController::new(sender.clone()));
        let game_operation_controller = Arc::new(GameOperationController::new(
            app_state_controller.clone(),
            sender.clone(),
        ));
        let slot_controller = Arc::new(SlotController::new(
            api_user_model.clone(),
            app_state_controller.clone(),
            sender.clone(),
        ));
        let game_controller = Arc::new(GameController::new(
            api_user_model.clone(),
            app_state_controller.clone(),
            config_controller.clone(),
            sender.clone(),
            image_controller.clone(),
            slot_controller.clone(),
            game_operation_controller.clone(),
        ));
        let ota_controller = Arc::new(OtaController::new(
            app_state_controller.clone(),
            sender.clone(),
        ));
        let session_controller = Arc::new(SessionController::new(
            api_user_model.clone(),
            app_state_controller.clone(),
            sender.clone(),
            game_controller.clone(),
            slot_controller.clone(),
            ota_controller.clone(),
        ));
        let user_controller = Arc::new(UserController::new(
            api_user_model.clone(),
            app_state_controller.clone(),
            sender.clone(),
        ));
        Self {
            api_user_model,
            app_state,
            app_state_controller,
            image_controller,
            config_controller,
            session_controller,
            game_controller,
            slot_controller,
            game_operation_controller,
            user_controller,
            ota_controller,
        }
    }

    pub fn attach(self: Arc<Self>, app: &AppWindow) -> UIMainThreadContext {
        app.on_register_requested(|| {
            ext_link::open_ext_link("https://closure.ltsc.vip");
        });
        app.on_open_ext_link(|str| {
            ext_link::open_ext_link(&str);
        });
        {
            let app_weak = app.as_weak();
            let this = self.clone();
            app.on_login_requested(move |account, password| {
                let app = app_weak.clone().unwrap();

                app.set_login_status_text(" 正在登录".into());
                app.set_login_state(LoginState::LoggingIn);

                let this = this.clone();
                tokio::spawn(async move {
                    this.session_controller
                        .login(account.into(), password.into())
                        .await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_auth_requested(move || {
                let this = this.clone();
                tokio::spawn(async move {
                    this.session_controller.auth().await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_load_logs(move |id, load_spec| {
                this.app_state_controller
                    .exec(|x| x.set_log_load_state(id.clone().into(), GameLogLoadState::Loading));

                let this = this.clone();
                tokio::spawn(async move {
                    this.game_controller
                        .retrieve_logs(id.into(), load_spec)
                        .await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_start_game(move |id| {
                this.app_state_controller.exec(|x| {
                    x.set_game_request_state(
                        id.clone().into(),
                        GameOperationRequestState::Requesting,
                    )
                });

                let this = this.clone();
                tokio::spawn(async move {
                    this.game_operation_controller.start_game(id.into()).await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_stop_game(move |id| {
                this.app_state_controller.exec(|x| {
                    x.set_game_request_state(
                        id.clone().into(),
                        GameOperationRequestState::Requesting,
                    )
                });

                let this = this.clone();
                tokio::spawn(async move {
                    this.game_operation_controller.stop_game(id.into()).await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_save_options(move |id, options| {
                this.app_state_controller.exec(|x| {
                    x.set_game_save_state(id.clone().into(), GameOptionSaveState::Saving)
                });

                let config_fields = GameOptionsMapping::from_ui(&options).to_game_options();

                let this = this.clone();
                tokio::spawn(async move {
                    this.game_controller
                        .update_game_settings(id.into(), config_fields)
                        .await;
                });
            })
        }

        {
            let this = self.clone();
            app.on_view_changed(move |id, view| {
                let this = this.to_owned();
                this.clone().app_state_controller.exec(|x| {
                    x.exec_with_game_by_id(
                        id.clone().into(),
                        move |game_info_list, i, mut game_info| {
                            game_info.active_view = view;
                            match view {
                                GameInfoViewType::DoctorInfo => {}
                                GameInfoViewType::Battle => {}
                                GameInfoViewType::Details => todo!(),
                                GameInfoViewType::Settings => {}
                                GameInfoViewType::Logs => {
                                    if game_info.log_loaded != GameLogLoadState::Loading {
                                        game_info.log_loaded = GameLogLoadState::Loading;
                                        let this = this.clone();
                                        tokio::spawn(async move {
                                            this.game_controller
                                                .retrieve_logs(
                                                    id.into(),
                                                    GameLogLoadRequestType::Later,
                                                )
                                                .await
                                        });
                                    }
                                }
                            }
                            game_info_list.set_row_data(i, game_info);
                        },
                    )
                });
            })
        }

        {
            let this = self.clone();
            app.on_reconnect_sse(move || {
                let this = this.to_owned();
                tokio::spawn(async move {
                    this.session_controller.spawn_sse_event_loop().await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_refresh_user_info(move || {
                let this = this.clone();
                tokio::spawn(async move {
                    this.slot_controller.refresh_slots().await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_update_slot(move |id, update_draft| {
                let this = this.clone();
                let update_request = SlotUpdateDraftMapping::from_ui(&update_draft);
                if let Some(update_request) = update_request {
                    let id = id.to_string();
                    tokio::spawn(async move {
                        this.slot_controller.update_slot(id, update_request).await;
                    });
                } else {
                    println!("[Controller] Unprocessable update draft: {update_draft:?}; please file a bug");
                }
            })
        }

        {
            let this = self.clone();
            app.on_reset_slot_update_request_state(move |id| {
                this.app_state_controller.exec(move |x| {
                    x.exec_with_slot_by_id(id.into(), move |slot_info_list, i, mut slot_info| {
                        slot_info.override_update_draft_type = SlotUpdateDraftType::Unchanged;
                        slot_info.update_request_state = SlotUpdateRequestState::Idle;
                        slot_info.update_result = SharedString::default();
                        slot_info_list.set_row_data(i, slot_info);
                    })
                });
            });
        }

        {
            let this = self.clone();
            app.on_slot_selected(move |id| {
                this.app_state_controller
                    .exec(|x| x.select_slot(id.into(), true));
            });
        }

        {
            let this = self.clone();
            app.on_expand_verify_slot(move || {
                let this = this.clone();

                tokio::spawn(async move {
                    for (id, slot_ref) in this.api_user_model.slot_map_read().await.iter() {
                        let availability_rank = slot_ref
                            .slot
                            .read()
                            .await
                            .data
                            .user_tier_availability_rank();
                        if (availability_rank & user_tier_availability_rank::TIER_BASIC) != 0 {
                            this.app_state_controller.exec(|x| {
                                x.exec_with_slot_by_id(
                                    id.into(),
                                    move |slot_info_list, i, mut slot_info| {
                                        slot_info.override_update_draft_type =
                                            SlotUpdateDraftType::Update;
                                        slot_info_list.set_row_data(i, slot_info);
                                    },
                                )
                            });
                            this.app_state_controller
                                .exec(|x| x.select_slot(id.clone(), false));
                            break;
                        }
                    }
                });
            })
        }

        {
            let this = self.clone();
            app.on_submit_sms_verify_code(move |code| {
                let this = this.clone();

                tokio::spawn(async move {
                    this.user_controller
                        .submit_sms_verify_code(code.into())
                        .await;
                    this.slot_controller.refresh_slots().await;
                });
            })
        }

        {
            let this = self.clone();
            app.on_fetch_qq_verify_code(move || {
                let this = this.clone();

                tokio::spawn(async move {
                    this.user_controller.get_qq_verify_code().await;
                });
            })
        }

        {
            let app_weak = app.as_weak();
            app.on_return_to_login_page(move || {
                app_weak.unwrap().invoke_do_return_to_login_page();
            });
        }

        {
            let this = self.clone();
            app.on_search_maps(move |id, term, fuzzy| {
                let this: Arc<UIContext> = this.clone();
                let term: String = term.trim().to_ascii_uppercase();
                if term.is_empty() || term == "-" {
                    return;
                }

                tokio::spawn(async move {
                    this.game_controller
                        .on_search_map(id.into(), term, fuzzy)
                        .await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_set_map_selected(move |id, battle_map, selected| {
                let this = this.clone();
                tokio::spawn(async move {
                    this.game_controller
                        .on_select_map(id.into(), battle_map.map_id.into(), selected)
                        .await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_reset_selected_maps(move |id| {
                let this = this.clone();
                tokio::spawn(async move {
                    this.game_controller.reset_selected_maps(id.into()).await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_save_maps(move |id, battle_update_fields| {
                this.app_state_controller.exec(|x| {
                    x.set_game_save_state(id.clone().into(), GameOptionSaveState::Saving)
                });

                let this = this.clone();
                let battle_maps = battle_update_fields
                    .maps
                    .iter()
                    .map(|x| x.map_id.into())
                    .collect();
                tokio::spawn(async move {
                    this.game_controller
                        .update_game_settings(
                            id.into(),
                            arkhost_api::models::api_arkhost::GameConfigFields {
                                battle_maps: Some(battle_maps),
                                ..Default::default()
                            },
                        )
                        .await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_load_screenshots(move |id| {
                let this = this.clone();
                tokio::spawn(async move {
                    this.game_controller
                        .load_battle_screenshots(id.as_str())
                        .await;
                });
            });
        }

        {
            let this = self.clone();
            app.on_download_update(move || {
                let this = this.clone();
                tokio::spawn(async move {
                    this.ota_controller.update_release().await;
                });
            })
        }

        {
            let this = self.clone();
            app.on_set_data_saver_mode(move |val| {
                this.config_controller.set_data_saver_mode_enabled(val);
            });
        }

        {
            let this = self.clone();
            app.on_set_clean_data(move |val| {
                _ = this
                    .config_controller
                    .set_clean_data(val)
                    .map_err(|e| println!("[Controller] error cleaning cache: {e}"));
            })
        }

        {
            let this = self.clone();
            app.on_recalculate_data_disk_usage(move || {
                this.config_controller.recalculate_disk_usage();
            })
        }

        {
            let this = self.clone();
            app.on_confirm_gacha_records(move || {
                let this = this.clone();
                tokio::spawn(async move {
                    this.game_controller.confirm_gacha_records().await;
                });
            })
        }

        let refresh_game_timer = {
            let app_weak = app.as_weak();
            let this = self.clone();
            let timer = slint::Timer::default();
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_secs(30),
                move || {
                    let app_weak = app_weak.clone();
                    let this = this.clone();
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        if app.get_login_state() != LoginState::Logged {
                            return;
                        }

                        let this = this.clone();
                        tokio::spawn(async move {
                            this.game_controller
                                .refresh_games(RefreshLogsCondition::OnLogsViewOpened)
                                .await;
                        });
                    })
                    .unwrap();
                },
            );
            timer
        };

        UIMainThreadContext { refresh_game_timer }
    }
}
