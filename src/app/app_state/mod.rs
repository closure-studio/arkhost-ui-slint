pub mod mapping;
pub mod model;

use self::{
    mapping::{GameInfoMapping, SlotInfoMapping, UserInfoMapping},
    model::{CharIllust, ImageData},
};
use super::ui::*;
use slint::{Model, ModelRc, Timer, VecModel, Weak};
use std::{rc::Rc, sync::Arc};
use tokio::sync::Notify;

pub struct AppState {
    pub ui: Weak<AppWindow>,
    pub refresh_game_timer: Timer,
}

pub struct AppStateAsyncOp {
    pub ui: Weak<AppWindow>,
    pub notify: Arc<Notify>,
    pub task: Box<dyn FnOnce(AppWindow) + Send + 'static>,
}

impl AppStateAsyncOp {
    pub fn create(
        ui: &Weak<AppWindow>,
        func: impl FnOnce(AppWindow) + Send + 'static,
    ) -> AppStateAsyncOp {
        let notify = Arc::new(Notify::new());

        AppStateAsyncOp {
            ui: ui.clone(),
            notify,
            task: Box::new(func),
        }
    }

    pub fn exec(self) {
        let task = self.task;
        self.ui
            .upgrade_in_event_loop(move |ui| {
                task(ui);
            })
            .unwrap();
    }

    pub async fn exec_wait(self) {
        let task = self.task;
        let notify = self.notify.clone();
        self.ui
            .upgrade_in_event_loop(move |ui| {
                task(ui);
                notify.notify_one();
            })
            .unwrap();
        self.notify.notified().await;
    }
}

impl AppState {
    pub fn new(ui: Weak<AppWindow>) -> Self {
        Self {
            ui,
            refresh_game_timer: Timer::default(),
        }
    }

    pub fn set_login_state(&self, state: LoginState, mut status_text: String) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            if !status_text.is_empty() {
                status_text.push(' '); // slint word wrap bug
            }
            ui.set_login_state(state);
            ui.set_login_status_text(status_text.into());
        })
    }

    pub fn set_fetch_games_state(&self, state: FetchGamesState) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            ui.set_fetch_games_state(state);
        })
    }

    pub fn set_sse_connect_state(&self, state: SseConnectState) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            ui.set_sse_connect_state(state);
        })
    }

    pub fn set_use_auth(&self, account: String, use_auth: bool) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            ui.invoke_set_use_auth(account.into(), use_auth);
        })
    }

    pub fn set_game_request_state(
        &self,
        id: String,
        state: GameOperationRequestState,
    ) -> AppStateAsyncOp {
        self.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
            game_info.request_state = state;
            game_info_list.set_row_data(i, game_info);
        })
    }

    pub fn set_game_save_state(&self, id: String, state: GameOptionSaveState) -> AppStateAsyncOp {
        self.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
            game_info.save_state = state;
            game_info_list.set_row_data(i, game_info);
        })
    }

    pub fn set_log_load_state(&self, id: String, state: GameLogLoadState) -> AppStateAsyncOp {
        self.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
            game_info.log_loaded = state;
            game_info_list.set_row_data(i, game_info);
        })
    }

    pub fn set_user_id_api_request_state(&self, state: UserIdApiRequestState) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            let mut user_info = ui.get_user_info();
            user_info.id_api_request_state = state;
            ui.set_user_info(user_info);
        })
    }

    pub fn set_slot_update_request_state(
        &self,
        id: String,
        state: SlotUpdateRequestState,
        status_text: Option<String>,
    ) -> AppStateAsyncOp {
        self.exec_with_slot_by_id(id, move |slot_info_list, i, mut slot_info| {
            slot_info.update_request_state = state;
            if let Some(mut status_text) = status_text {
                if !status_text.is_empty() {
                    status_text.push(' '); // slint word wrap bug
                }
                slot_info.update_result = status_text.into();
            }

            slot_info_list.set_row_data(i, slot_info);
        })
    }

    pub fn set_game_images(
        &self,
        id: String,
        avatar: Option<ImageData>,
        char_illust: Option<CharIllust>,
    ) -> AppStateAsyncOp {
        self.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
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

    // TODO: update_slot_info_lost中逻辑与该部分重复
    pub fn update_game_views(
        &self,
        mut game_list: Vec<(i32, String, GameInfoMapping)>,
        update_logs: bool,
    ) -> AppStateAsyncOp {
        game_list.sort_by_key(|(order, _, _)| *order);
        self.exec_in_event_loop(move |ui| {
            let game_info_list = ui.get_game_info_list();
            if game_list.len() == game_info_list.row_count()
                && game_info_list
                    .iter()
                    .enumerate()
                    .all(|(i, x)| x.id == game_list[i].1)
            {
                game_info_list.iter().enumerate().for_each(|(i, mut x)| {
                    let (_, _, game_info_mapping) = &game_list[i];
                    game_info_mapping.mutate(&mut x, update_logs);
                    game_info_list.set_row_data(i, x);
                });
                return;
            }

            let game_info_list: Vec<GameInfo> = game_list
                .iter()
                .map(|(_, _, mapping)| mapping.create_game_info())
                .collect();
            ui.set_game_info_list(Rc::new(VecModel::from(game_info_list)).into());
            println!("[AppState] Recreated rows on game list changed");
        })
    }

    pub fn update_game_view(
        &self,
        id: String,
        mapping: Option<GameInfoMapping>,
        update_logs: bool,
    ) -> AppStateAsyncOp {
        self.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
            if update_logs {
                game_info.log_loaded = GameLogLoadState::Loaded;
            }
            if let Some(mapping) = mapping {
                mapping.mutate(&mut game_info, update_logs);
            }
            game_info_list.set_row_data(i, game_info);
        })
    }
    pub fn exec_with_game_by_id(
        &self,
        id: String,
        func: impl FnOnce(ModelRc<GameInfo>, usize, GameInfo) + Send + 'static,
    ) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            let game_info_list = ui.get_game_info_list();
            match Self::find_game_by_id(&game_info_list, &id) {
                Some((i, game_info)) => func(game_info_list, i, game_info),
                None => {
                    println!("[AppState] Game not found: {id:?}");
                }
            }
        })
    }

    pub fn select_slot(&self, id: String, toggle: bool) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            let slot_info_list = ui.get_slot_info_list();

            for (i, mut slot_info) in slot_info_list.iter().enumerate() {
                if slot_info.uuid == id && slot_info.view_state != SlotDetailsViewState::Independent
                {
                    slot_info.view_state =
                        if !toggle || (slot_info.view_state != SlotDetailsViewState::Expanded) {
                            SlotDetailsViewState::Expanded
                        } else {
                            SlotDetailsViewState::Collapsed
                        }
                }

                slot_info_list.set_row_data(i, slot_info);
            }
        })
    }

    pub fn update_slot_info_list(
        &self,
        mut slot_list: Vec<(i32, String, SlotInfoMapping)>,
    ) -> AppStateAsyncOp {
        slot_list.sort_by_key(|(order, _, _)| *order);
        self.exec_in_event_loop(move |ui| {
            let slot_info_list = ui.get_slot_info_list();
            if slot_list.len() == slot_info_list.row_count()
                && slot_info_list
                    .iter()
                    .enumerate()
                    .all(|(i, x)| x.uuid == slot_list[i].1)
            {
                slot_info_list.iter().enumerate().for_each(|(i, mut x)| {
                    let (_, _, slot_info_mapping) = &slot_list[i];
                    slot_info_mapping.mutate(&mut x);
                    slot_info_list.set_row_data(i, x);
                });
                return;
            }

            let slot_info_list: Vec<SlotInfo> = slot_list
                .iter()
                .map(|(_, _, mapping)| mapping.create_slot_info())
                .collect();
            ui.set_slot_info_list(Rc::from(VecModel::from(slot_info_list)).into());
            println!("[AppState] Recreated rows on slot list changed");
        })
    }

    pub fn update_slot_info(&self, uuid: String, mapping: SlotInfoMapping) -> AppStateAsyncOp {
        self.exec_with_slot_by_id(uuid, move |slot_info_list, i, mut slot_info| {
            mapping.mutate(&mut slot_info);
            slot_info_list.set_row_data(i, slot_info);
        })
    }

    pub fn exec_with_slot_by_id(
        &self,
        id: String,
        func: impl FnOnce(ModelRc<SlotInfo>, usize, SlotInfo) + Send + 'static,
    ) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            let slot_info_list = ui.get_slot_info_list();
            match Self::find_slot_by_id(&slot_info_list, &id) {
                Some((i, slot_info)) => {
                    func(slot_info_list, i, slot_info);
                }
                None => {
                    println!("[AppState] Slot not found: {id:?}");
                }
            }
        })
    }

    pub fn update_user_info(&self, mapping: UserInfoMapping) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            let mut user_info = ui.get_user_info();
            mapping.mutate(&mut user_info);
            ui.set_user_info(user_info);
        })
    }

    pub fn state_globals(
        &self,
        func: impl FnOnce(StateGlobals<'_>) + Send + 'static,
    ) -> AppStateAsyncOp {
        self.exec_in_event_loop(move |ui| {
            func(ui.global::<StateGlobals>());
        })
    }

    pub fn exec_in_event_loop(
        &self,
        func: impl FnOnce(AppWindow) + Send + 'static,
    ) -> AppStateAsyncOp {
        AppStateAsyncOp::create(&self.ui, func)
    }

    fn find_game_by_id(game_info_list: &ModelRc<GameInfo>, id: &str) -> Option<(usize, GameInfo)> {
        game_info_list
            .iter()
            .enumerate()
            .find(|(_i, x)| x.id.as_str() == id)
            .take()
    }

    fn find_slot_by_id(slot_info_list: &ModelRc<SlotInfo>, id: &str) -> Option<(usize, SlotInfo)> {
        slot_info_list
            .iter()
            .enumerate()
            .find(|(_i, x)| x.uuid.as_str() == id)
            .take()
    }
}
