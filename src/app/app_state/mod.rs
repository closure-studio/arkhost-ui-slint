pub mod model;

use self::model::{CharIllust, GameInfoModel, ImageData};
use super::ui::*;
use slint::{Model, ModelRc, Timer, VecModel, Weak};
use std::rc::Rc;

pub struct AppState {
    pub ui: Weak<AppWindow>,
    pub refresh_game_timer: Timer,
}

impl AppState {
    pub fn new(ui: Weak<AppWindow>) -> Self {
        Self {
            ui,
            refresh_game_timer: Timer::default(),
        }
    }

    pub fn set_login_state(&self, state: LoginState, mut status_text: String) {
        self.ui
            .upgrade_in_event_loop(move |ui| {
                status_text.push(' '); // slint word wrap bug
                ui.set_login_state(state);
                ui.set_login_status_text(status_text.into());
            })
            .unwrap();
    }

    pub fn set_fetch_games_state(&self, state: FetchGamesState) {
        self.ui
            .upgrade_in_event_loop(move |ui| {
                ui.set_fetch_games_state(state);
            })
            .unwrap();
    }

    pub fn set_use_auth(&self, account: String, use_auth: bool) {
        self.ui
            .upgrade_in_event_loop(move |ui| {
                ui.invoke_set_use_auth(account.into(), use_auth);
            })
            .unwrap();
    }

    pub fn set_game_request_state(&self, id: String, state: GameOperationRequestState) {
        self.get_game_by_id(id, move |game_info_list, i, mut game_info| {
            game_info.request_state = state;
            game_info_list.set_row_data(i, game_info);
        });
    }

    pub fn set_game_save_state(&self, id: String, state: GameOptionSaveState) {
        self.get_game_by_id(id, move |game_info_list, i, mut game_info| {
            game_info.save_state = state;
            game_info_list.set_row_data(i, game_info);
        });
    }

    pub fn set_log_load_state(&self, id: String, state: GameLogLoadState) {
        self.get_game_by_id(id, move |game_info_list, i, mut game_info| {
            game_info.log_loaded = state;
            game_info_list.set_row_data(i, game_info);
        });
    }

    pub fn set_game_images(
        &self,
        id: String,
        avatar: Option<ImageData>,
        char_illust: Option<CharIllust>,
    ) {
        self.get_game_by_id(id, move |game_info_list, i, mut game_info| {
            if let Some(avatar) = avatar {
                if let Some(image) = avatar.to_slint_image() {
                    game_info.details.avatar.loaded = true;
                    game_info.details.avatar.avatar_image = image;
                }
            }

            if let Some(illust_data) = char_illust {
                if let Some(image) = illust_data.image.to_slint_image() {
                    let illust = &mut game_info.details.char_illust;
                    let positions = &illust_data.positions;
                    illust.loaded = true;
                    illust.illust_image = image;
                    [illust.pivot_factor_x, illust.pivot_factor_y] = positions.pivot_factor;
                    [illust.offset_x, illust.offset_y] = positions.pivot_offset;
                    [illust.scale_x, illust.scale_y] = positions.scale;
                    [illust.size_x, illust.size_y] = positions.size;
                }
            }

            game_info_list.set_row_data(i, game_info);
        })
    }

    pub fn update_game_views(
        &self,
        mut game_list: Vec<(i32, String, GameInfoModel)>,
        update_logs: bool,
    ) {
        game_list.sort_by_key(|(order, _, _)| *order);
        self.ui
            .upgrade_in_event_loop(move |ui| {
                let current_game_info_list = ui.get_game_info_list();
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
                            game_info_represent.mutate(&mut x, update_logs);
                            current_game_info_list.set_row_data(i, x);
                        });
                    return;
                }

                let game_info_list: Vec<GameInfo> = game_list
                    .iter()
                    .map(|(_, _, rep)| rep.create_game_info())
                    .collect();
                let model = Rc::new(VecModel::from(game_info_list));
                ui.set_game_info_list(ModelRc::from(model));
                println!("[AppState] Recreated rows on game list changed");
            })
            .unwrap();
    }

    pub fn update_game_view(&self, id: String, model: Option<GameInfoModel>, update_logs: bool) {
        self.get_game_by_id(id, move |game_info_list, i, mut game_info| {
            if update_logs {
                game_info.log_loaded = GameLogLoadState::Loaded;
            }
            if let Some(model) = model {
                model.mutate(&mut game_info, update_logs);
            }
            game_info_list.set_row_data(i, game_info);
        })
    }

    fn find_game_by_id(game_info_list: &ModelRc<GameInfo>, id: &str) -> Option<(usize, GameInfo)> {
        game_info_list
            .iter()
            .enumerate()
            .find(|(_i, x)| x.id.as_str() == id)
            .take()
    }

    pub fn get_game_by_id(
        &self,
        id: String,
        func: impl FnOnce(ModelRc<GameInfo>, usize, GameInfo) + Send + 'static,
    ) {
        self.ui
            .upgrade_in_event_loop(move |ui| {
                let game_info_list = ui.get_game_info_list();
                match AppState::find_game_by_id(&game_info_list, &id) {
                    Some((i, game_info)) => func(game_info_list, i, game_info),
                    None => { /* report error here */ }
                }
            })
            .unwrap();
    }
}
