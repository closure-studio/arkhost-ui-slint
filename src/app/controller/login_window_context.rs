use super::app_state_controller::AppStateController;
use super::config_controller::ConfigController;
use super::session_controller::SessionController;
use crate::app::app_state::LoginWindowState;
use crate::app::LoginState;
use crate::app::{ui::LoginWindow, utils::ext_link};
use slint::ComponentHandle;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

pub struct LoginWindowContext {
    pub login_window_ref: Rc<OnceLock<LoginWindow>>,
    pub login_window_state: Arc<Mutex<LoginWindowState>>,
    pub session_controller: Arc<SessionController>,

    app_state_controller: Arc<AppStateController>,
    config_controller: Arc<ConfigController>,
}

impl LoginWindowContext {
    pub fn new(
        login_window_ref: Rc<OnceLock<LoginWindow>>,
        login_window_state: Arc<Mutex<LoginWindowState>>,
        app_state_controller: Arc<AppStateController>,
        session_controller: Arc<SessionController>,
        config_controller: Arc<ConfigController>,
    ) -> Self {
        Self {
            login_window_ref,
            login_window_state,
            app_state_controller,
            session_controller,
            config_controller,
        }
    }

    pub fn load_login_window(&self) -> Arc<Mutex<LoginWindowState>> {
        self.login_window_ref.get_or_init(|| {
            let login_window = LoginWindow::new().expect("创建登录窗口失败！");

            login_window.on_set_data_saver_mode({
                let config_controller = self.config_controller.clone();

                move |val| {
                    config_controller.set_data_saver_mode_enabled(val);
                }
            });

            login_window.on_open_ext_link(|str| {
                ext_link::open_ext_link(&str);
            });

            login_window.on_login_requested({
                let login_window_state = self.login_window_state.clone();
                let app_state_controller = self.app_state_controller.clone();
                let session_controller = self.session_controller.clone();

                move |account, password| {
                    login_window_state
                        .lock()
                        .unwrap()
                        .set_login_state(LoginState::LoggingIn, "".into());

                    let login_window_state = login_window_state.clone();
                    let app_state_controller = app_state_controller.clone();
                    let session_controller = session_controller.clone();
                    tokio::spawn(async move {
                        if let Err(e) = session_controller
                            .authorize_with_account(account.into(), password.into())
                            .await
                        {
                            login_window_state
                                .lock()
                                .unwrap()
                                .set_login_state(LoginState::Errored, format!("{e:?}"));
                        } else {
                            app_state_controller.exec_wait(|x| x.show()).await;
                            login_window_state.lock().unwrap().hide();
                            session_controller.create_user_model().await;
                            session_controller.on_post_create_user_model().await;
                        }
                    });
                }
            });

            login_window.on_auth_requested({
                let login_window_state = self.login_window_state.clone();
                let app_state_controller = self.app_state_controller.clone();
                let session_controller = self.session_controller.clone();

                move || {
                    login_window_state
                        .lock()
                        .unwrap()
                        .set_login_state(LoginState::LoggingIn, "".into());

                    let login_window_state = login_window_state.clone();
                    let app_state_controller = app_state_controller.clone();
                    let session_controller = session_controller.clone();
                    tokio::spawn(async move {
                        if let Err(e) = session_controller.authorize_with_stored_token().await {
                            login_window_state
                                .lock()
                                .unwrap()
                                .set_login_state(LoginState::Errored, format!("{e:?}"));
                        } else {
                            app_state_controller.exec_wait(|x| x.show()).await;
                            login_window_state.lock().unwrap().hide();
                            session_controller.create_user_model().await;
                            session_controller.on_post_create_user_model().await;
                        }
                    });
                }
            });

            self.login_window_state
                .lock()
                .unwrap()
                .assign_login_window(login_window.as_weak());
            login_window
        });

        self.login_window_state.clone()
    }
}
