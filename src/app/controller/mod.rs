mod model;
extern crate alloc;

use self::model::{GameInfoRepresent, GameOptionsRepresent};

use super::api_controller::{Command as ApiCommand, RetrieveLogSpec};
use super::auth_controller::Command as AuthCommand;

use super::ui::*;
use super::utils::ext_link;
use super::webview::auth::AuthResult;
use anyhow::anyhow;
use arkhost_api::clients::common::ApiResult;
use arkhost_api::models::api_arkhost::GameConfigFields;
use slint::{Model, ModelRc, VecModel, Weak};
use std::fmt::Debug;
use std::rc::Rc;
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
    timers: Vec<slint::Timer>,
}

impl Controller {
    pub fn new() -> Self {
        Self { timers: vec![] }
    }

    pub fn attach(
        &mut self,
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

            app.on_login_requested(move |account, password| {
                let app_weak = app_weak.clone();
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

                tokio::task::spawn(async move {
                    Controller::login(
                        app_weak.clone(),
                        account.into(),
                        password.into(),
                        send.clone(),
                    )
                    .await;
                });
            });
        }

        {
            let app_weak = app.as_weak();
            let send = tx_api_controller.clone();

            app.on_load_logs(move |id, load_spec| {
                let app = app_weak.clone().unwrap();
                let game_info_list = app.get_game_info_list();
                match Controller::find_game_by_id(&game_info_list, &id) {
                    Some((i, mut game_info)) => {
                        game_info.log_loaded = GameLogLoadState::Loading;
                        game_info_list.set_row_data(i, game_info);
                    }
                    None => eprintln!("Game with id {id} not found in game info list"),
                }

                tokio::task::spawn(Controller::retrieve_logs(
                    app_weak.clone(),
                    id.into(),
                    load_spec,
                    send.clone(),
                ));
            });
        }

        {
            let app_weak = app.as_weak();
            let tx_api_controller = tx_api_controller.clone();
            let tx_auth_controller = tx_auth_controller.clone();

            app.on_start_game(move |id| {
                let app = app_weak.clone().unwrap();
                let game_info_list = app.get_game_info_list();
                match Controller::find_game_by_id(&game_info_list, &id) {
                    Some((i, mut game_info)) => {
                        game_info.request_state = GameOperationRequestState::Requesting;
                        game_info_list.set_row_data(i, game_info);
                    }
                    None => eprintln!("Game with id {id} not found in game info list"),
                }

                tokio::task::spawn(Controller::start_game(
                    app_weak.clone(),
                    id.into(),
                    tx_api_controller.clone(),
                    tx_auth_controller.clone(),
                ));
            });
        }

        {
            let app_weak = app.as_weak();
            let tx_api_controller = tx_api_controller.clone();

            app.on_stop_game(move |id| {
                let app = app_weak.clone().unwrap();
                let game_info_list = app.get_game_info_list();
                match Controller::find_game_by_id(&game_info_list, &id) {
                    Some((i, mut game_info)) => {
                        game_info.request_state = GameOperationRequestState::Requesting;
                        game_info_list.set_row_data(i, game_info);
                    }
                    None => eprintln!("Game with id {id} not found in game info list"),
                }

                tokio::task::spawn(Controller::stop_game(
                    app_weak.clone(),
                    id.into(),
                    tx_api_controller.clone(),
                ));
            });
        }

        {
            let app_weak = app.as_weak();
            let tx_api_controller = tx_api_controller.clone();
            
            app.on_save_options(move |id| {
                let app = app_weak.clone().unwrap();
                let game_info_list = app.get_game_info_list();
                match Controller::find_game_by_id(&game_info_list, &id) {
                    Some((i, mut game_info)) => {
                        game_info.save_state = GameOptionSaveState::Saving;
                        let config_fields = GameOptionsRepresent::from_ui(&game_info.options).to_game_options();
                        game_info_list.set_row_data(i, game_info);

                        tokio::task::spawn(Controller::update_game_settings(
                            app_weak.clone(),
                            id.into(),
                            config_fields,
                            tx_api_controller.clone(),
                        ));
                    }
                    None => eprintln!("Game with id {id} not found in game info list"),
                }
            })
        }

        {
            let app_weak = app.as_weak();
            let send = tx_api_controller.clone();

            app.on_view_changed(move |id, view| {
                let app = app_weak.clone().unwrap();
                let game_info_list = app.get_game_info_list();
                match Controller::find_game_by_id(&game_info_list, &id) {
                    Some((i, mut game_info)) => {
                        game_info.active_view = view;
                        match view {
                            GameInfoViewType::DoctorInfo => {}
                            GameInfoViewType::Details => todo!(),
                            GameInfoViewType::Settings => {}
                            GameInfoViewType::Logs => {
                                if game_info.log_loaded == GameLogLoadState::NotLoaded {
                                    game_info.log_loaded = GameLogLoadState::Loading;
                                    tokio::task::spawn(Controller::retrieve_logs(
                                        app_weak.clone(),
                                        id.into(),
                                        GameLogLoadRequestType::Later,
                                        send.clone(),
                                    ));
                                }
                            }
                        }
                        game_info_list.set_row_data(i, game_info);
                    }
                    None => eprintln!("Game with id {id} not found in game info list"),
                }
            })
        }

        {
            let app_weak = app.as_weak();
            let send = tx_api_controller.clone();

            let timer = slint::Timer::default();
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_secs(10),
                move || {
                    let app_weak = app_weak.clone();
                    let send = send.clone();
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        if app.get_login_state() != LoginState::Logged {
                            return;
                        }

                        tokio::task::spawn(Controller::refresh_games(
                            app_weak,
                            send,
                            RefreshLogsCondition::OnLogsViewOpened,
                        ));
                    })
                    .unwrap();
                },
            );
            self.timers.push(timer);
        }
    }

    pub async fn login(
        app_weak: Weak<AppWindow>,
        account: String,
        password: String,
        tx: mpsc::Sender<ApiCommand>,
    ) {
        let (resp, rx) = oneshot::channel();
        {
            let app_weak = app_weak.clone();
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
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        app.set_login_status_text("登陆成功".into());
                        app.set_login_state(LoginState::Logged);
                    })
                    .unwrap();
                }
                Err(e) => {
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        app.set_login_status_text(format!("{:?}", e).into());
                        app.set_login_state(LoginState::Errored);
                    })
                    .unwrap();
                }
            }
        }
        Controller::refresh_games(app_weak, tx, RefreshLogsCondition::OnLogsViewOpened).await;
    }

    pub async fn auth(app_weak: Weak<AppWindow>, tx: mpsc::Sender<ApiCommand>) {
        {
            let app_weak = app_weak.clone();
            slint::invoke_from_event_loop(move || {
                let app = app_weak.unwrap();
                app.set_login_status_text("自动登陆中……".into());
            })
            .unwrap();
        }
        let (resp, rx) = oneshot::channel();
        {
            let app_weak = app_weak.clone();
            match Controller::send_request(ApiCommand::Auth { resp }, tx.clone(), rx).await {
                Ok(user) => {
                    println!(
                        "[Controller] Auth success: {} {}",
                        user.user_email, user.uuid
                    );
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        app.set_login_status_text("登陆成功".into());
                        app.set_login_state(LoginState::Logged);
                    })
                    .unwrap();
                }
                Err(e) => {
                    slint::invoke_from_event_loop(move || {
                        let app = app_weak.unwrap();
                        app.set_login_status_text(
                            format!("登陆认证已失效，请重新登陆\n{:?}", e).into(),
                        );
                        app.set_login_state(LoginState::Errored);
                    })
                    .unwrap();
                }
            }
        }
        Controller::refresh_games(app_weak, tx, RefreshLogsCondition::OnLogsViewOpened).await;
    }

    pub async fn refresh_games(
        app_weak: Weak<AppWindow>,
        tx: mpsc::Sender<ApiCommand>,
        refresh_log_cond: RefreshLogsCondition,
    ) {
        let (resp, rx) = oneshot::channel();
        match Controller::send_request(ApiCommand::RetrieveGames { resp }, tx.clone(), rx).await {
            Ok(games) => {
                let mut game_list: Vec<(i32, String, GameInfoRepresent)> = Vec::new();
                for game_ref in games.read().await.values() {
                    let game_info = game_ref.info.read().await;
                    game_list.push((
                        game_ref.order,
                        game_info.info.status.account.clone(),
                        GameInfoRepresent::from(&game_info),
                    ));
                }
                game_list.sort_by_key(|(order, _, _)| *order);

                slint::invoke_from_event_loop(move || {
                    let app = app_weak.clone().unwrap();
                    Controller::refresh_game_views(app, &game_list, false);
                    game_list.iter().for_each(|(_, id, _)| {
                        Controller::refresh_logs_if_needed(
                            app_weak.clone(),
                            id.clone(),
                            refresh_log_cond.clone(),
                            tx.clone(),
                        );
                    })
                })
                .unwrap();
            }
            Err(e) => {
                println!("[Controller] Error retrieving games {}", e);
            }
        };
    }

    pub async fn retrieve_logs(
        app_weak: Weak<AppWindow>,
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
                let game_represent = GameInfoRepresent::from(game);
                Controller::refresh_game_view(app_weak, id, Some(game_represent), true);
            }
            Err(e) => {
                eprintln!("[Controller] error retrieving logs for game with id {id}: {e}");
                Controller::refresh_game_view(app_weak, id, None, true);
            }
        }
    }

    pub async fn start_game(
        app_weak: Weak<AppWindow>,
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
            match Controller::try_start_game(
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
        _ = tx_auth_controller.send(AuthCommand::HideWindow {  }).await;

        if !success {
            eprintln!("[Controller] all attempts to start game {account} failed");
        }
        Controller::reset_game_request_state(app_weak.clone(), account.clone());
        Controller::refresh_games(app_weak, tx_api_controller, RefreshLogsCondition::Never).await;
    }

    pub async fn stop_game(
        app_weak: Weak<AppWindow>,
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
                Controller::refresh_games(
                    app_weak.clone(),
                    tx_api_controller,
                    RefreshLogsCondition::Never,
                )
                .await
            }
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        Controller::reset_game_request_state(app_weak.clone(), account.clone());
    }

    pub async fn update_game_settings(
        app_weak: Weak<AppWindow>,
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
                Controller::refresh_games(
                    app_weak.clone(),
                    tx_api_controller,
                    RefreshLogsCondition::Never,
                )
                .await
            }
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        Controller::reset_game_save_state(app_weak.clone(), account.clone());
    }

    async fn try_start_game(
        account: String,
        auth_command: AuthCommand,
        tx_api_controller: mpsc::Sender<ApiCommand>,
        tx_auth_controller: mpsc::Sender<AuthCommand>,
        rx_auth_controller: oneshot::Receiver<anyhow::Result<AuthResult>>,
    ) -> anyhow::Result<()> {
        let auth_result =
            Controller::send_auth_request(auth_command, tx_auth_controller.clone(), rx_auth_controller)
                .await?;
        let captcha_token = match auth_result {
            AuthResult::ArkHostCaptchaTokenReCaptcha { token, .. } => token,
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

    fn reset_game_request_state(app_weak: Weak<AppWindow>, id: String) {
        slint::invoke_from_event_loop(move || {
            let app = app_weak.clone().unwrap();
            let game_info_list = app.get_game_info_list();
            match Controller::find_game_by_id(&game_info_list, &id) {
                Some((i, mut game_info)) => {
                    game_info.request_state = GameOperationRequestState::Idle;
                    game_info_list.set_row_data(i, game_info);
                }
                None => eprintln!("Game with id {id} not found in game info list"),
            }
        })
        .unwrap();
    }

    fn reset_game_save_state(app_weak: Weak<AppWindow>, id: String) {
        slint::invoke_from_event_loop(move || {
            let app = app_weak.clone().unwrap();
            let game_info_list = app.get_game_info_list();
            match Controller::find_game_by_id(&game_info_list, &id) {
                Some((i, mut game_info)) => {
                    game_info.save_state = GameOptionSaveState::Idle;
                    game_info_list.set_row_data(i, game_info);
                }
                None => eprintln!("Game with id {id} not found in game info list"),
            }
        })
        .unwrap();
    }

    fn refresh_logs_if_needed(
        app_weak: Weak<AppWindow>,
        id: String,
        cond: RefreshLogsCondition,
        tx: mpsc::Sender<ApiCommand>,
    ) {
        slint::invoke_from_event_loop(move || {
            let app = app_weak.clone().unwrap();
            let game_info_list = app.get_game_info_list();
            match Controller::find_game_by_id(&game_info_list, &id) {
                Some((i, mut game_info)) => {
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
                        tokio::task::spawn(Controller::retrieve_logs(
                            app_weak.clone(),
                            id.into(),
                            load_spec,
                            tx,
                        ));
                    }
                }
                None => eprintln!("Game with id {id} not found in game info list"),
            }
        })
        .unwrap();
    }

    fn refresh_game_view(
        app_weak: Weak<AppWindow>,
        id: String,
        game_represent: Option<GameInfoRepresent>,
        refresh_logs: bool,
    ) {
        slint::invoke_from_event_loop(move || {
            let app = app_weak.unwrap();
            let game_info_list = app.get_game_info_list();
            match Controller::find_game_by_id(&game_info_list, &id) {
                Some((i, mut game_info)) => {
                    if refresh_logs {
                        game_info.log_loaded = GameLogLoadState::Loaded;
                    }
                    if let Some(game_represent) = game_represent {
                        game_represent.mutate(&mut game_info, refresh_logs);
                    }
                    game_info_list.set_row_data(i, game_info);
                }
                None => eprintln!("Game with id {id} not found in game info list"),
            }
        })
        .unwrap();
    }

    fn refresh_game_views(
        app: AppWindow,
        game_list: &Vec<(i32, String, GameInfoRepresent)>,
        refresh_logs: bool,
    ) {
        let current_game_info_list = app.get_game_info_list();
        if game_list.len() == current_game_info_list.row_count()
            && current_game_info_list
                .iter()
                .enumerate()
                .all(|(i, x)| x.id == &game_list[i].1)
        {
            current_game_info_list
                .iter()
                .enumerate()
                .for_each(|(i, mut x)| {
                    let (_, _, game_info_represent) = &game_list[i];
                    game_info_represent.mutate(&mut x, refresh_logs);
                    current_game_info_list.set_row_data(i, x);
                });
            return;
        }

        let game_info_list: Vec<GameInfo> = game_list
            .iter()
            .map(|(_, _, rep)| rep.create_game_info())
            .collect();
        let model = Rc::new(VecModel::from(game_info_list));
        app.set_game_info_list(ModelRc::from(model));
        println!("[Controller] Recreated rows on game list changed");
    }

    fn find_game_by_id(game_info_list: &ModelRc<GameInfo>, id: &str) -> Option<(usize, GameInfo)> {
        game_info_list
            .iter()
            .enumerate()
            .find(|(_i, x)| x.id.as_str() == id)
            .take()
    }

    async fn send_command(command: ApiCommand, tx: mpsc::Sender<ApiCommand>) -> ApiResult<()> {
        tx.send(command)
            .await
            .map_err(ApiError::CommandSendError::<ApiCommand>)?;
        Ok(())
    }

    ///
    /// * T: 返回类型
    ///
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
