extern crate alloc;

use super::app_state::model::{GameInfoModel, GameOptionsModel};
use super::app_state::AppState;

use super::api_controller::{Command as ApiCommand, RetrieveLogSpec};
use super::auth_controller::Command as AuthCommand;

use super::ui::*;
use super::utils::ext_link;
use super::webview::auth::AuthResult;
use anyhow::anyhow;
use arkhost_api::clients::common::ApiResult;
use arkhost_api::models::api_arkhost::GameConfigFields;
use slint::Model;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

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

pub struct Controller {
    app_state: Arc<Mutex<AppState>>,
}

impl Controller {
    pub fn new(app_state: AppState) -> Self {
        Self {
            app_state: Arc::new(Mutex::new(app_state)),
        }
    }

    pub fn attach(
        self: Arc<Self>,
        app: &AppWindow,
        tx_api_controller: mpsc::Sender<ApiCommand>,
        tx_auth_controller: mpsc::Sender<AuthCommand>,
    ) {
        app.on_register_requested(|| {
            ext_link::open_ext_link("https://arknights.host");
        });
        app.on_open_ext_link(|str| {
            ext_link::open_ext_link(&str);
        });
        {
            let app_weak = app.as_weak();
            let send = tx_api_controller.clone();
            let this = self.clone();

            app.on_login_requested(move |account, password| {
                let send = send.clone();
                let app = app_weak.clone().unwrap();

                if account.len() == 0 {
                    app.set_login_status_text("账号不能为空".into());
                    app.set_login_state(LoginState::Errored);
                    return;
                }

                if password.len() == 0 {
                    app.set_login_status_text("密码不能为空".into());
                    app.set_login_state(LoginState::Errored);
                    return;
                }

                app.set_login_status_text("正在登陆……".into());
                app.set_login_state(LoginState::LoggingIn);

                tokio::task::spawn(this.to_owned().login(
                    account.into(),
                    password.into(),
                    send.clone(),
                ));
            });
        }

        {
            let send = tx_api_controller.clone();
            let this = self.clone();

            app.on_load_logs(move |id, load_spec| {
                this.app_state
                    .lock()
                    .unwrap()
                    .set_log_load_state(id.clone().into(), GameLogLoadState::Loading);

                tokio::task::spawn(this.to_owned().retrieve_logs(
                    id.into(),
                    load_spec,
                    send.clone(),
                ));
            });
        }

        {
            let tx_api_controller = tx_api_controller.clone();
            let tx_auth_controller = tx_auth_controller.clone();
            let this = self.clone();

            app.on_start_game(move |id| {
                this.app_state.lock().unwrap().set_game_request_state(
                    id.clone().into(),
                    GameOperationRequestState::Requesting,
                );
                tokio::task::spawn(this.to_owned().start_game(
                    id.into(),
                    tx_api_controller.clone(),
                    tx_auth_controller.clone(),
                ));
            });
        }

        {
            let tx_api_controller = tx_api_controller.clone();
            let this = self.clone();

            app.on_stop_game(move |id| {
                this.app_state.lock().unwrap().set_game_request_state(
                    id.clone().into(),
                    GameOperationRequestState::Requesting,
                );
                tokio::task::spawn(
                    this.to_owned()
                        .stop_game(id.into(), tx_api_controller.clone()),
                );
            });
        }

        {
            let tx_api_controller = tx_api_controller.clone();
            let this = self.clone();

            app.on_save_options(move |id, options| {
                this.app_state
                    .lock()
                    .unwrap()
                    .set_game_save_state(id.clone().into(), GameOptionSaveState::Saving);

                let config_fields = GameOptionsModel::from_ui(&options).to_game_options();

                tokio::task::spawn(this.to_owned().update_game_settings(
                    id.into(),
                    config_fields,
                    tx_api_controller.clone(),
                ));
            })
        }

        {
            let send = tx_api_controller.clone();
            let this = self.clone();

            app.on_view_changed(move |id, view| {
                let send = send.clone();
                let this = this.to_owned();
                this.clone().app_state.lock().unwrap().get_game_by_id(
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
                                    tokio::task::spawn(this.retrieve_logs(
                                        id.into(),
                                        GameLogLoadRequestType::Later,
                                        send.clone(),
                                    ));
                                }
                            }
                        }
                        game_info_list.set_row_data(i, game_info);
                    },
                );
            })
        }

        {
            let app_weak = app.as_weak();
            let send = tx_api_controller.clone();
            let this = self.clone();

            let timer = slint::Timer::default();
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_secs(10),
                move || {
                    let app_weak = app_weak.clone();
                    let send = send.clone();
                    let this = this.clone();
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        if app.get_login_state() != LoginState::Logged {
                            return;
                        }

                        tokio::task::spawn(
                            this.to_owned()
                                .refresh_games(send, RefreshLogsCondition::OnLogsViewOpened),
                        );
                    })
                    .unwrap();
                },
            );
            self.app_state.lock().unwrap().refresh_game_timer = timer;
        }
    }

    pub async fn login(
        self: Arc<Self>,
        account: String,
        password: String,
        tx: mpsc::Sender<ApiCommand>,
    ) {
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(
            ApiCommand::Login {
                email: account.into(),
                password: password.into(),
                resp,
            },
            tx.clone(),
            rx,
        )
        .await
        {
            Ok(user) => {
                println!("[Controller] Logged in: {} {}", user.user_email, user.uuid);
                self.app_state
                    .lock()
                    .unwrap()
                    .set_login_state(LoginState::Logged, "登陆成功".into());
            }
            Err(e) => {
                self.app_state
                    .lock()
                    .unwrap()
                    .set_login_state(LoginState::Errored, format!("{:?}", e).into());
            }
        }
        self.refresh_games(tx, RefreshLogsCondition::Never).await;
    }

    pub async fn auth(self: Arc<Self>, tx: mpsc::Sender<ApiCommand>) {
        self.app_state
            .lock()
            .unwrap()
            .set_login_state(LoginState::LoggingIn, "自动登录中……".into());
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(ApiCommand::Auth { resp }, tx.clone(), rx).await {
            Ok(user) => {
                println!(
                    "[Controller] Auth success: {} {}",
                    user.user_email, user.uuid
                );
                self.app_state
                    .lock()
                    .unwrap()
                    .set_login_state(LoginState::Logged, "登录成功".into());
            }
            Err(e) => {
                self.app_state.lock().unwrap().set_login_state(
                    LoginState::Errored,
                    format!("登陆认证已失效，请重新登陆\n{:?}", e).into(),
                );
            }
        }
        self.refresh_games(tx, RefreshLogsCondition::OnLogsViewOpened)
            .await;
    }

    pub async fn refresh_games(
        self: Arc<Self>,
        tx: mpsc::Sender<ApiCommand>,
        refresh_log_cond: RefreshLogsCondition,
    ) {
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(ApiCommand::RetrieveGames { resp }, tx.clone(), rx).await {
            Ok(games) => {
                let mut game_list: Vec<(i32, String, GameInfoModel)> = Vec::new();
                for game_ref in games.read().await.values() {
                    let game_info = game_ref.info.read().await;
                    game_list.push((
                        game_ref.order,
                        game_info.info.status.account.clone(),
                        GameInfoModel::from(&game_info),
                    ));
                }
                game_list.sort_by_key(|(order, _, _)| *order);

                let game_ids: Vec<String> = game_list.iter().map(|(_, id, _)| id.clone()).collect();
                self.app_state
                    .lock()
                    .unwrap()
                    .update_game_views(game_list, false);
                for id in game_ids {
                    let this = self.clone();
                    tokio::task::spawn(this.refresh_logs_if_needed(
                        id.into(),
                        refresh_log_cond.clone(),
                        tx.clone(),
                    ));
                }
            }
            Err(e) => {
                println!("[Controller] Error retrieving games {}", e);
            }
        };
    }

    pub async fn retrieve_logs(
        self: Arc<Self>,
        id: String,
        load_spec: GameLogLoadRequestType,
        tx: mpsc::Sender<ApiCommand>,
    ) {
        let spec = match load_spec {
            GameLogLoadRequestType::Later => RetrieveLogSpec::Latest {},
            GameLogLoadRequestType::Former => RetrieveLogSpec::Former {},
        };
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(
            ApiCommand::RetrieveLog {
                account: id.clone(),
                spec,
                resp,
            },
            tx,
            rx,
        )
        .await
        {
            Ok(game_ref) => {
                let game = &game_ref.info.read().await;
                let model = GameInfoModel::from(game);
                self.app_state
                    .lock()
                    .unwrap()
                    .update_game_view(id, Some(model), true);
            }
            Err(e) => {
                eprintln!("[Controller] error retrieving logs for game with id {id}: {e}");
                self.app_state
                    .lock()
                    .unwrap()
                    .update_game_view(id, None, true);
            }
        }
    }

    pub async fn start_game(
        self: Arc<Self>,
        account: String,
        tx_api_controller: mpsc::Sender<ApiCommand>,
        tx_auth_controller: mpsc::Sender<AuthCommand>,
    ) {
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

        for (auth_command, rx) in auth_methods {
            match self
                .try_start_game(
                    account.clone(),
                    auth_command,
                    tx_api_controller.clone(),
                    tx_auth_controller.clone(),
                    rx,
                )
                .await
            {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => println!("[Controller] failed attempting to start game {account}: {e}"),
            }
        }
        _ = tx_auth_controller.send(AuthCommand::HideWindow {}).await;

        if !success {
            eprintln!("[Controller] all attempts to start game {account} failed");
        }
        self.app_state
            .lock()
            .unwrap()
            .set_game_request_state(account.clone(), GameOperationRequestState::Idle);
        self.refresh_games(tx_api_controller, RefreshLogsCondition::Never)
            .await;
    }

    pub async fn stop_game(
        self: Arc<Self>,
        account: String,
        tx_api_controller: mpsc::Sender<ApiCommand>,
    ) {
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(
            ApiCommand::StopGame {
                account: account.clone(),
                resp,
            },
            tx_api_controller.clone(),
            rx,
        )
        .await
        {
            Ok(_) => {
                self.clone()
                    .refresh_games(tx_api_controller, RefreshLogsCondition::Never)
                    .await
            }
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        self.app_state
            .lock()
            .unwrap()
            .set_game_request_state(account.clone(), GameOperationRequestState::Idle);
    }

    pub async fn update_game_settings(
        self: Arc<Self>,
        account: String,
        config_fields: GameConfigFields,
        tx_api_controller: mpsc::Sender<ApiCommand>,
    ) {
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(
            ApiCommand::UpdateGameSettings {
                account: account.clone(),
                config: config_fields,
                resp,
            },
            tx_api_controller.clone(),
            rx,
        )
        .await
        {
            Ok(_) => {
                self.clone()
                    .refresh_games(tx_api_controller, RefreshLogsCondition::Never)
                    .await
            }
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        self.app_state
            .lock()
            .unwrap()
            .set_game_save_state(account.clone(), GameOptionSaveState::Idle);
    }

    async fn try_start_game(
        &self,
        account: String,
        auth_command: AuthCommand,
        tx_api_controller: mpsc::Sender<ApiCommand>,
        tx_auth_controller: mpsc::Sender<AuthCommand>,
        rx_auth_controller: oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<()> {
        let auth_result = Controller::send_auth_request(
            auth_command,
            tx_auth_controller.clone(),
            rx_auth_controller,
        )
        .await?;
        let captcha_token = match auth_result {
            AuthResult::ArkHostCaptchaTokenReCaptcha { token, .. } => token,
            AuthResult::ArkHostCaptchaTokenGeeTest { token, .. } => token,
            _ => {
                return Err(anyhow!("unexpected auth result: {auth_result:?}").into());
            }
        };

        let (resp, rx) = oneshot::channel();
        Controller::send_request(
            ApiCommand::StartGame {
                account,
                captcha_token,
                resp,
            },
            tx_api_controller,
            rx,
        )
        .await
        .map(|_| ())
    }

    async fn refresh_logs_if_needed(
        self: Arc<Self>,
        id: String,
        cond: RefreshLogsCondition,
        tx: mpsc::Sender<ApiCommand>,
    ) {
        let this = self.clone();
        self.app_state.lock().unwrap().get_game_by_id(
            id.clone(),
            move |game_info_list, i, mut game_info| {
                let should_refresh = match cond {
                    RefreshLogsCondition::Always => true,
                    RefreshLogsCondition::OnLogsViewOpened => {
                        game_info.log_loaded == GameLogLoadState::Loaded
                            && game_info.active_view == GameInfoViewType::Logs
                    }
                    RefreshLogsCondition::Never => false,
                };
                if should_refresh && game_info.log_loaded != GameLogLoadState::Loading {
                    game_info.log_loaded = GameLogLoadState::Loading;
                    game_info_list.set_row_data(i, game_info);
                    let load_spec = GameLogLoadRequestType::Later;
                    tokio::task::spawn(this.to_owned().retrieve_logs(id.into(), load_spec, tx));
                }
            },
        )
    }

    async fn send_command(command: ApiCommand, tx: mpsc::Sender<ApiCommand>) -> ApiResult<()> {
        tx.send(command)
            .await
            .map_err(ApiError::CommandSendError::<ApiCommand>)?;
        Ok(())
    }

    async fn send_request<T>(
        command: ApiCommand,
        tx: mpsc::Sender<ApiCommand>,
        rx: oneshot::Receiver<ApiResult<T>>,
    ) -> ApiResult<T>
    where
        T: 'static + Send + Sync + Debug,
    {
        Controller::send_command(command, tx).await?;
        let recv = rx.await.map_err(ApiError::<T>::RespRecvError)?;
        match recv {
            Ok(resp) => Ok(resp),
            Err(e) => Err(e),
        }
    }

    async fn send_auth_request(
        command: AuthCommand,
        tx: mpsc::Sender<AuthCommand>,
        rx: oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<AuthResult> {
        tx.send(command).await?;
        let auth_res = rx.await?;
        Ok(auth_res?)
    }
}
