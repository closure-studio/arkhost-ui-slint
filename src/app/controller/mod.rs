pub mod account_controller;
pub mod api_model;
pub mod app_state_controller;
pub mod game_controller;
pub mod game_operation_controller;
pub mod image_controller;
pub mod request_controller;
extern crate alloc;

use self::api_model::ApiModel;
use self::game_controller::GameController;
use self::game_operation_controller::GameOperationController;
use self::image_controller::ImageController;
use self::request_controller::RequestController;
use self::{account_controller::AccountController, app_state_controller::AppStateController};
use super::app_state::model::GameOptionsModel;
use super::app_state::AppState;
use super::ui::*;
use super::utils::ext_link;
use slint::Model;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

type ApiOperation = super::api_controller::Operation;
type ApiCommand = super::api_controller::Command;
type AuthCommand = super::auth_controller::Command;
type AssetCommand = super::asset_controller::Command;

type ApiResult<T> = arkhost_api::clients::common::ApiResult<T>;
type AuthResult = super::webview::auth::AuthResult;
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

pub struct ControllerHub {
    pub app_state: Arc<Mutex<AppState>>,
    pub app_state_controller: Arc<AppStateController>,
    pub image_controller: Arc<ImageController>,
    pub account_controller: Arc<AccountController>,
    pub game_controller: Arc<GameController>,
    pub game_operation_controller: Arc<GameOperationController>,
}

impl ControllerHub {
    pub fn new(
        app_state: AppState,
        api_model: Arc<ApiModel>,
        tx_api_controller: mpsc::Sender<ApiCommand>,
        tx_auth_controller: mpsc::Sender<AuthCommand>,
        tx_asset_controller: mpsc::Sender<AssetCommand>,
    ) -> Self {
        let app_state = Arc::new(Mutex::new(app_state));
        let app_state_controller = Arc::new(AppStateController {
            app_state: app_state.clone(),
        });
        let request_controller = Arc::new(RequestController {
            api_model: api_model.clone(),
            tx_api_controller,
            tx_auth_controller,
            tx_asset_controller,
        });
        let image_controller = Arc::new(ImageController::new(request_controller.clone()));
        let game_operation_controller = Arc::new(GameOperationController::new(
            app_state_controller.clone(),
            request_controller.clone(),
        ));
        let game_controller = Arc::new(GameController::new(
            api_model,
            app_state_controller.clone(),
            request_controller.clone(),
            image_controller.clone(),
            game_operation_controller.clone(),
        ));
        let account_controller = Arc::new(AccountController::new(
            app_state_controller.clone(),
            request_controller.clone(),
            game_controller.clone(),
        ));
        Self {
            app_state: app_state.clone(),
            app_state_controller,
            image_controller,
            account_controller,
            game_controller,
            game_operation_controller,
        }
    }

    pub fn attach(self: Arc<Self>, app: &AppWindow) {
        app.on_register_requested(|| {
            ext_link::open_ext_link("https://www.arknights.host");
        });
        app.on_open_ext_link(|str| {
            ext_link::open_ext_link(&str);
        });
        {
            let app_weak = app.as_weak();
            let this = self.clone();

            app.on_login_requested(move |account, password| {
                let app = app_weak.clone().unwrap();

                if account.is_empty() {
                    app.set_login_status_text("账号不能为空 ".into());
                    app.set_login_state(LoginState::Errored);
                    return;
                }

                if password.is_empty() {
                    app.set_login_status_text("密码不能为空 ".into());
                    app.set_login_state(LoginState::Errored);
                    return;
                }

                app.set_login_status_text("正在登录…… ".into());
                app.set_login_state(LoginState::LoggingIn);

                let this = this.clone();
                tokio::spawn(async move {
                    this.account_controller
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
                    this.account_controller.auth().await;
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

                let config_fields = GameOptionsModel::from_ui(&options).to_game_options();

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
                    this.account_controller.start_sse_event_loop().await;
                });
            });
        }

        {
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
            self.app_state.lock().unwrap().refresh_game_timer = timer;
        }
    }
}
