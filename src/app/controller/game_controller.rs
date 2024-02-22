use crate::app::{
    api_worker::RetrieveLogSpec,
    app_state::{
        mapping::{BattleMapMapping, GameInfoMapping},
        model::{CharIllust, ImageDataRaw, ImageDataRef},
    },
    asset_worker::AssetRef,
    controller::RefreshLogsCondition,
    game_data::{CharPack, CharPackSummaryTable, Stage, StageDropType, StageTable, StageType},
    rt_api_model,
    ui::*,
    utils::{
        levenshtein_distance::{self, ResultEntry},
        notification,
    },
};
use anyhow::anyhow;
use arkhost_api::models::api_arkhost::{self, GameConfigFields, GameSseEvent, GameStatus};
use futures_util::{future::join_all, TryStreamExt};
use serde::Deserialize;
use slint::{Model, ModelRc, VecModel};
use tokio::{
    join,
    sync::{oneshot, RwLock},
};
use tokio_util::sync::CancellationToken;

use std::{
    cmp,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{
    app_state_controller::AppStateController, game_operation_controller::GameOperationController,
    image_controller::ImageController, rt_api_model::RtApiModel, sender::Sender,
    slot_controller::SlotController, ApiOperation, AssetCommand,
};

#[derive(Default, Debug)]
struct GameResourceEntry {
    pub avatar: ImageDataRef,
    pub char_illust: ImageDataRef,
    pub char_illust_filename: RwLock<Option<String>>,
}

pub struct GameController {
    game_resource_map: RwLock<HashMap<String, Arc<GameResourceEntry>>>,
    #[allow(unused)] // TODO: 关卡相关
    stage_data: RwLock<Option<StageTable>>,
    stage_search_tree: RwLock<levenshtein_distance::Trie<Arc<String>>>,
    char_pack_summaries: RwLock<Option<CharPackSummaryTable>>,
    refreshing: AtomicBool,

    rt_api_model: Arc<RtApiModel>,
    app_state_controller: Arc<AppStateController>,
    sender: Arc<Sender>,
    image_controller: Arc<ImageController>,
    slot_controller: Arc<SlotController>,
    game_operation_controller: Arc<GameOperationController>,
}

impl GameController {
    pub fn new(
        rt_api_model: Arc<RtApiModel>,
        app_state_controller: Arc<AppStateController>,
        sender: Arc<Sender>,
        image_controller: Arc<ImageController>,
        slot_controller: Arc<SlotController>,
        game_operation_controller: Arc<GameOperationController>,
    ) -> Self {
        Self {
            game_resource_map: RwLock::new(HashMap::new()),
            stage_data: RwLock::new(None),
            stage_search_tree: Default::default(),
            char_pack_summaries: RwLock::new(None),
            refreshing: AtomicBool::new(false),

            rt_api_model,
            app_state_controller,
            sender,
            image_controller,
            slot_controller,
            game_operation_controller,
        }
    }

    pub async fn refresh_games(&self, refresh_log_cond: super::RefreshLogsCondition) {
        if self
            .refreshing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        self.try_ensure_resources().await;
        let (resp, mut rx) = oneshot::channel();
        self.app_state_controller
            .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetching));
        match self
            .sender
            .send_api_request(ApiOperation::RetrieveGames { resp }, &mut rx)
            .await
        {
            Ok(_) => {
                self.slot_controller.submit_slot_model_to_ui().await;
                self.process_game_details().await;
                self.process_game_list_changes(refresh_log_cond).await;
                self.app_state_controller
                    .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetched));
            }
            Err(e) => {
                println!("[Controller] Error retrieving games {e}");
                self.app_state_controller
                    .exec(|x| x.set_fetch_games_state(FetchGamesState::Retrying));
            }
        };

        self.refreshing.store(false, Ordering::Release);
    }

    pub async fn run_sse_event_loop(&self, stop: CancellationToken) -> anyhow::Result<()> {
        self.app_state_controller
            .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetching));
        let (resp, rx) = oneshot::channel();
        self.sender
            .send_api_command(ApiOperation::ConnectGameEventSource { resp })
            .await?;

        self.app_state_controller
            .exec(|x| x.set_sse_connect_state(SseConnectState::Connected));

        let mut stream = rx.await??;
        tokio::select! {
            _ = async {
                let mut is_initial = true;
                loop {
                    match stream.try_next().await {
                        Ok(Some(ev)) => match ev {
                            GameSseEvent::Game(games) => {
                                println!("[Controller] Games SSE connection received {} games", games.len());

                                self.rt_api_model.user.handle_retrieve_games_result(games).await;
                                self.slot_controller.submit_slot_model_to_ui().await;
                                self.process_game_details().await;
                                self.process_game_list_changes(RefreshLogsCondition::Never).await;

                                if is_initial {
                                    is_initial = false;
                                    self.app_state_controller
                                        .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetched));
                                }
                            },
                            GameSseEvent::Ssr(ssr_list) => {
                                println!("[Controller] Games SSE connection received {} ssr records", ssr_list.len());
                            },
                            GameSseEvent::Close => {
                                println!("[Controller] Games SSE connection closed on occupied");
                                self.app_state_controller
                                    .exec(|x| x.set_sse_connect_state(SseConnectState::DisconnectedOccupiedElsewhere));
                                break;
                            },
                            GameSseEvent::Unrecognized(ev_type) => {
                                println!("[Controller] Unrecognized SSE event: {ev_type}");
                            },
                            GameSseEvent::RecoverableError(e) => {
                                println!("[Controller] SSE Client is recovering on error: {e}")
                            }
                        },
                        Ok(None) => {
                            eprintln!("[Controller] Unexpected empty event in game SSE connection");
                            /* 处理None? */
                        },
                        Err(e) => {
                            self.app_state_controller
                                .exec(|x| x.set_sse_connect_state(SseConnectState::Disconnected));
                            eprintln!("[Controller] Error in game SSE connection: {e:?}");
                            break;
                        }
                    }
                }
            } => {},
            _ = stop.cancelled() => {},
        };

        println!(
            "[Controller] Games SSE connection terminated with stop signal raised: {}",
            stop.is_cancelled()
        );
        Ok(())
    }

    pub async fn process_game_details(&self) {
        let stage_data = self.stage_data.read().await;
        let mut games_to_fetch_details: Vec<String> = Vec::new();
        for game_ref in self.rt_api_model.game_map_read().await.values() {
            let mut game = game_ref.game.write().await;
            if let Some(stage) = stage_data.as_ref().and_then(|t| {
                game.info
                    .game_config
                    .map_id
                    .as_ref()
                    .and_then(|m| t.stages.get(m))
            }) {
                let mut stage_name = stage.display();
                stage_name.insert(0, ' ');
                stage_name.insert_str(0, &stage.code);
                game.stage_name = Some(stage_name);
            }

            if game.info.status.code == api_arkhost::GameStatus::Running {
                games_to_fetch_details.push(game.info.status.account.clone());
            }
        }
        let tasks = games_to_fetch_details
            .into_iter()
            .map(|account| async move {
                let (resp, mut rx) = oneshot::channel();
                let result = self
                    .sender
                    .send_api_request(
                        ApiOperation::RetrieveGameDetails {
                            account: account.clone(),
                            resp,
                        },
                        &mut rx,
                    )
                    .await;
                if let Err(e) = result {
                    println!(
                        "[Controller] Error retrieving detail for game '{}' {}",
                        account, e
                    );
                }
            });
        join_all(tasks).await;
    }

    pub async fn process_game_list_changes(&self, refresh_log_cond: super::RefreshLogsCondition) {
        let mut game_list: Vec<(i32, String, GameInfoMapping)> = Vec::new();
        let game_map = self.rt_api_model.game_map_read().await;

        let mut load_images_task = vec![];
        for game_ref in game_map.values() {
            let game = game_ref.game.read().await;
            if game.info.status.code == GameStatus::Captcha {
                let account = game.info.status.account.clone();
                let gt = game.info.captcha_info.gt.clone();
                let challenge = game.info.captcha_info.challenge.clone();
                _ = self
                    .game_operation_controller
                    .try_preform_game_captcha(account, gt, challenge)
                    .await;
            }

            game_list.push((
                game_ref.order.load(Ordering::Acquire),
                game.info.status.account.clone(),
                GameInfoMapping::from(&game),
            ));

            load_images_task.push(async {
                let game = game_ref.game.read().await;
                self.try_ensure_game_images(&game, game.info.status.account.clone())
                    .await;
            });
        }

        game_list.sort_by_key(|(order, _, _)| *order);
        let game_ids: Vec<String> = game_list.iter().map(|(_, id, _)| id.clone()).collect();
        self.app_state_controller
            .exec_wait(|x| x.update_game_views(game_list, false))
            .await;
        for id in game_ids.clone() {
            let game_ref = game_map.get(&id);
            if let Some(game_ref) = game_ref {
                let game_entry = game_ref.game.read().await;
                self.set_selected_maps(id.clone(), &game_entry.info, false)
                    .await;
            }
        }

        join_all(load_images_task).await;
        let mut refresh_log_tasks = vec![];
        for id in game_ids {
            refresh_log_tasks
                .push(self.refresh_logs_if_needed(id.clone(), refresh_log_cond.clone()));
            self.try_apply_game_images(id).await;
        }
        join_all(refresh_log_tasks).await;
    }

    pub async fn update_game_settings(&self, account: String, config_fields: GameConfigFields) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(
                ApiOperation::UpdateGameSettings {
                    account: account.clone(),
                    config: config_fields,
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(_) => {}
            Err(e) => {
                eprintln!("[Controller] Error update game settings {e}");
                notification::toast(
                    &format!("{account} 更新托管设置失败"),
                    None,
                    &format!("{e}"),
                    None,
                );
            }
        }

        self.app_state_controller
            .exec(|x| x.set_game_save_state(account.clone(), GameOptionSaveState::Idle));
    }

    pub async fn retrieve_logs(&self, id: String, load_spec: GameLogLoadRequestType) {
        let spec = match load_spec {
            GameLogLoadRequestType::Later => RetrieveLogSpec::Latest {},
            GameLogLoadRequestType::Former => RetrieveLogSpec::Former {},
        };
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_api_request(
                ApiOperation::RetrieveLog {
                    account: id.clone(),
                    spec,
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(game_ref) => {
                let game = &game_ref.game.read().await;
                let mapping = GameInfoMapping::from(game);
                self.app_state_controller
                    .exec(|x| x.update_game_view(id, Some(mapping), true));
            }
            Err(e) => {
                eprintln!("[Controller] error retrieving logs for game with id {id}: {e}");
                notification::toast(&format!("{id} 获取日志失败"), None, &format!("{e}"), None);
                self.app_state_controller
                    .exec(|x| x.update_game_view(id, None, true));
            }
        }
    }

    // TODO: 模糊搜索优化 & 关键词搜索
    pub async fn on_search_map(&self, id: String, term: String, fuzzy: bool) {
        let search_tree = self.stage_search_tree.read().await;
        let battle_map_mappings = match self.stage_data.read().await.as_ref() {
            Some(stage_data) if !term.is_empty() => {
                let chars: Vec<char> = term.chars().collect();
                let results: Vec<ResultEntry<Arc<String>>> = search_tree
                    .search(
                        if fuzzy {
                            consts::SEARCH_MAP_MAX_LEVENSHTEIN_DISTANCE_FUZZY
                        } else {
                            consts::SEARCH_MAP_MAX_LEVENSHTEIN_DISTANCE
                        },
                        &chars,
                    )
                    .into_iter()
                    .collect();
                let mut stages: Vec<(i32, &String, &Stage)> = results
                    .iter()
                    .filter_map(|ResultEntry(dist, map_id)| {
                        stage_data
                            .stages
                            .get(map_id.as_ref())
                            .map(|s| (*dist, map_id.as_ref(), s))
                    })
                    .collect();
                stages.sort_by(
                    |(lhs_dist, lhs_map_id, lhs_stage), (rhs_dist, rhs_map_id, rhs_stage)| {
                        match (
                            lhs_stage.name.as_ref().and_then(|x| x.find(&term)),
                            rhs_stage.name.as_ref().and_then(|x| x.find(&term)),
                        ) {
                            (Some(_), None) => return cmp::Ordering::Less,
                            (None, Some(_)) => return cmp::Ordering::Greater,
                            (Some(lhs), Some(rhs)) if lhs != rhs => return lhs.cmp(&rhs),
                            _ => {}
                        };

                        match (lhs_stage.code.find(&term), rhs_stage.code.find(&term)) {
                            (Some(_), None) => return cmp::Ordering::Less,
                            (None, Some(_)) => return cmp::Ordering::Greater,
                            (Some(lhs), Some(rhs)) if lhs != rhs => return lhs.cmp(&rhs),
                            _ => {}
                        };

                        match lhs_dist.cmp(rhs_dist) {
                            cmp::Ordering::Equal => {}
                            ord => return ord,
                        }

                        match lhs_stage.cmp(rhs_stage) {
                            cmp::Ordering::Equal => lhs_map_id.cmp(rhs_map_id),
                            ord => ord,
                        }
                    },
                );
                stages.truncate(consts::SEARCH_MAP_RESULT_LIMIT);
                stages
                    .into_iter()
                    .map(|(_, map_id, stage)| {
                        BattleMapMapping {
                            map_id: map_id.clone(),
                            code_name: stage.code.clone(),
                            display_name: stage.display(),
                        }
                        .create_battle_map()
                    })
                    .collect()
            }
            _ => vec![],
        };

        self.app_state_controller.exec(move |x| {
            x.exec_with_game_by_id(id, |game_info_list, i, mut game_info| {
                game_info.map_search_results = ModelRc::new(VecModel::from(battle_map_mappings));
                game_info_list.set_row_data(i, game_info);
            })
        });
    }

    pub async fn on_select_map(&self, id: String, map_id: String, selected: bool) {
        let battle_map = if selected {
            self.stage_data
                .read()
                .await
                .as_ref()
                .and_then(|x| x.stages.get(&map_id))
                .map(|x| {
                    BattleMapMapping {
                        map_id: map_id.clone(),
                        code_name: x.code.clone(),
                        display_name: x.display(),
                    }
                    .create_battle_map()
                })
        } else {
            None
        };

        self.app_state_controller.exec(move |x| {
            x.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
                if game_info
                    .selected_maps
                    .as_any()
                    .downcast_ref::<VecModel<BattleMap>>()
                    .is_none()
                {
                    game_info.selected_maps = ModelRc::new(VecModel::from(vec![]));
                }

                let selected_maps_rc = &game_info.selected_maps;
                if let Some(selected_maps) = selected_maps_rc
                    .as_any()
                    .downcast_ref::<VecModel<BattleMap>>()
                {
                    if let Some((map_index, _)) = selected_maps
                        .iter()
                        .enumerate()
                        .find(|(_, x)| x.map_id == map_id)
                    {
                        selected_maps.remove(map_index);
                    }

                    if let Some(battle_map) = battle_map {
                        selected_maps.push(battle_map);
                    }
                }

                game_info_list.set_row_data(i, game_info);
            })
        })
    }

    pub async fn reset_selected_maps(&self, id: String) {
        let game_map = self.rt_api_model.game_map_read().await;
        let game_ref = game_map.get(&id);
        if let Some(game_ref) = game_ref {
            let game_entry = game_ref.game.read().await;
            self.set_selected_maps(id, &game_entry.info, true).await;
        }
    }

    pub async fn set_selected_maps(
        &self,
        id: String,
        game_info: &api_arkhost::GameInfo,
        override_existing: bool,
    ) {
        let mut battle_maps_to_set = vec![];
        let stage_data = self.stage_data.read().await;
        if let Some(battle_maps) = &game_info.game_config.battle_maps {
            for map_id in battle_maps {
                let battle_map = stage_data
                    .as_ref()
                    .and_then(|x| x.stages.get(map_id))
                    .map_or_else(
                        || {
                            BattleMapMapping {
                                map_id: map_id.clone(),
                                code_name: "".into(),
                                display_name: map_id.clone(),
                            }
                            .create_battle_map()
                        },
                        |x| {
                            BattleMapMapping {
                                map_id: map_id.clone(),
                                code_name: x.code.clone(),
                                display_name: x.display(),
                            }
                            .create_battle_map()
                        },
                    );

                battle_maps_to_set.push(battle_map);
            }
        }

        self.app_state_controller.exec(|x| {
            x.exec_with_game_by_id(id, move |game_info_list, i, mut game_info| {
                if override_existing || game_info.selected_maps.row_count() == 0 {
                    game_info.selected_maps = ModelRc::new(VecModel::from(battle_maps_to_set));
                    game_info_list.set_row_data(i, game_info);
                }
            })
        });
    }

    pub async fn try_ensure_game_images(&self, game: &rt_api_model::GameEntry, id: String) {
        let game_resource_entry;
        {
            let mut game_resource_map = self.game_resource_map.write().await;
            if let Some(resource_entry) = game_resource_map.get(&id) {
                game_resource_entry = resource_entry.clone();
            } else {
                game_resource_entry = Arc::new(GameResourceEntry::default());
                _ = game_resource_map.insert(id, game_resource_entry.clone());
            }
        }
        let image_controller = &self.image_controller;
        let load_game_avatar =
            image_controller.load_game_avatar_if_empty(game, game_resource_entry.avatar.clone());
        let load_game_image = image_controller
            .load_game_char_illust_if_empty(game, game_resource_entry.char_illust.clone());

        if let Some(details) = &game.details {
            if !details.status.secretary_skin_id.is_empty() {
                _ = game_resource_entry
                    .char_illust_filename
                    .write()
                    .await
                    .insert(details.status.sanitize_secretary_skin_id_for_url());
            }
        }

        join!(load_game_avatar, load_game_image);
    }

    pub async fn try_apply_game_images(&self, id: String) {
        if let Some(resource_entry) = self.game_resource_map.read().await.get(&id) {
            let mut avatar_image_data = None;
            let mut char_illust_data = None;

            let image_data = resource_entry.avatar.read().await;
            if let ImageDataRaw::Rgba8 { .. } = &image_data.loaded_image {
                avatar_image_data = Some(image_data.clone());
            }

            let image_data = resource_entry.char_illust.read().await;
            if let ImageDataRaw::Rgba8 { .. } = &image_data.loaded_image {
                if let Some(ref char_illust_filename) =
                    *resource_entry.char_illust_filename.read().await
                {
                    let char_pack = self
                        .char_pack_summaries
                        .read()
                        .await
                        .as_ref()
                        .and_then(|x| x.get(char_illust_filename))
                        .cloned()
                        .unwrap_or_else(|| {
                            CharPack {
                                name: "".into(),
                                pivot_factor: [0.5, 0.5],
                                pivot_offset: [0., 0.],
                                scale: [0.7, 0.7], // 不确定pivot的情况下尽可能缩小图像，以免细节被遮挡
                                size: [100., 100.],
                            }
                        });
                    char_illust_data = Some(CharIllust {
                        image: image_data.clone(),
                        positions: char_pack.clone(),
                    })
                }
            }

            if avatar_image_data.is_some() || char_illust_data.is_some() {
                self.app_state_controller
                    .exec(|x| x.set_game_images(id, avatar_image_data, char_illust_data));
            }
        }
    }

    pub async fn try_ensure_resources(&self) {
        let has_stage_data = self.stage_data.read().await.is_some();
        let has_char_pack_summary = self.char_pack_summaries.read().await.is_some();

        tokio::join!(
            async {
                if !has_stage_data {
                    let path = arkhost_api::consts::asset::api::gamedata("excel/stage_table.json");
                    let stage_data = self.load_json_table::<StageTable>(path, None).await;
                    if let Some(stage_data) = stage_data {
                        self.build_stage_search_tree(&stage_data).await;
                        _ = self.stage_data.write().await.insert(stage_data);
                    }
                }
            },
            async {
                if !has_char_pack_summary {
                    let path = arkhost_api::consts::asset::api::charpack("summary.json");
                    let char_pack_summary = self
                        .load_json_table::<CharPackSummaryTable>(path, None)
                        .await;
                    if let Some(char_pack_summary) = char_pack_summary {
                        _ = self
                            .char_pack_summaries
                            .write()
                            .await
                            .insert(char_pack_summary);
                    }
                }
            }
        );
    }

    async fn load_json_table<T>(&self, path: String, cache_key: Option<String>) -> Option<T>
    where
        T: 'static + Send + Sync + for<'de> Deserialize<'de>,
    {
        let (resp, mut rx) = oneshot::channel();
        match self
            .sender
            .send_asset_request(
                AssetCommand::LoadAsset {
                    cache_key,
                    path: path.clone(),
                    resp,
                },
                &mut rx,
            )
            .await
            .and_then(|x| match x {
                AssetRef::Bytes(bytes) => Ok(bytes),
                _ => Err(anyhow!(format!("unexpected AssetRef: {x:?}"))),
            })
            .and_then(|x| serde_json::de::from_slice::<T>(&x).map_err(anyhow::Error::from))
        {
            Ok(res) => Some(res),
            Err(e) => {
                eprintln!("[GameController] Error loading resource '{path}' {e:?}");
                None
            }
        }
    }

    async fn refresh_logs_if_needed(&self, id: String, cond: super::RefreshLogsCondition) {
        let should_refresh_val = Arc::new(AtomicBool::new(false));

        {
            let should_refresh_val = should_refresh_val.clone();
            self.app_state_controller
                .exec_wait(|x| {
                    x.exec_with_game_by_id(id.clone(), move |game_info_list, i, mut game_info| {
                        let should_refresh = match cond {
                            super::RefreshLogsCondition::Always => true,
                            super::RefreshLogsCondition::OnLogsViewOpened => {
                                game_info.log_loaded == GameLogLoadState::Loaded
                                    && game_info.active_view == GameInfoViewType::Logs
                            }
                            super::RefreshLogsCondition::Never => false,
                        };
                        if should_refresh && game_info.log_loaded != GameLogLoadState::Loading {
                            game_info.log_loaded = GameLogLoadState::Loading;
                            game_info_list.set_row_data(i, game_info);
                            should_refresh_val.store(true, Ordering::Release);
                        }
                    })
                })
                .await;
        }

        if should_refresh_val.load(Ordering::Acquire) {
            self.retrieve_logs(id, GameLogLoadRequestType::Later).await;
        }
    }

    async fn build_stage_search_tree(&self, stage_data: &StageTable) {
        let mut stage_search_tree = self.stage_search_tree.write().await;
        stage_search_tree.clear();

        let mut entries: u32 = 0;
        for (id, stage) in
            stage_data.stages.iter().filter(|(_, stage)| {
                stage.can_battle_replay
                    && !matches!(
                        stage.stage_type,
                        StageType::Training
                            | StageType::Guide
                            | StageType::SpecialStory
                            | StageType::Campaign
                            | StageType::HandbookBattle
                            | StageType::ClimbTower
                    )
                    && stage.stage_drop_info.display_rewards.iter().any(|x| {
                        matches!(x.drop_type, StageDropType::Normal | StageDropType::Special)
                    })
                    && stage.ap_cost > 0
                    && !stage.is_predefined
                    && !stage.is_hard_predefined
                    && !stage.is_skill_selectable_predefined
                    && !stage.is_story_only
            })
        {
            let id: Arc<String> = Arc::new(id.clone());
            if !stage.code.is_empty() {
                let code_name: Vec<char> = stage.code.chars().collect();
                stage_search_tree.insert(&code_name, id.clone());
                entries += 1;
            }
            if let Some(name) = &stage.name {
                let name: Vec<char> = name.chars().collect();
                stage_search_tree.insert(&name, id);
                entries += 1;
            }
        }

        println!("[Controller] Inserted {entries} entries into stage search tree");
    }
}

mod consts {
    pub const SEARCH_MAP_MAX_LEVENSHTEIN_DISTANCE: i32 = 1;
    pub const SEARCH_MAP_MAX_LEVENSHTEIN_DISTANCE_FUZZY: i32 = 5;
    pub const SEARCH_MAP_RESULT_LIMIT: usize = 50;
}
