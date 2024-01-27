use crate::app::{
    api_controller::RetrieveLogSpec,
    api_model,
    app_state::model::{CharIllust, GameInfoModel, ImageDataRaw, ImageDataRef},
    asset_controller::AssetRef,
    controller::RefreshLogsCondition,
    game_data::{CharPack, CharPackSummaryTable, StageTable},
    ui::*,
};
use anyhow::anyhow;
use arkhost_api::models::api_arkhost::{self, GameConfigFields, GameSseEvent, GameStatus};
use futures_util::{future::join_all, TryStreamExt};
use serde::Deserialize;
use slint::Model;
use tokio::{
    join,
    sync::{oneshot, RwLock},
};
use tokio_util::sync::CancellationToken;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{
    api_model::ApiModel, app_state_controller::AppStateController,
    game_operation_controller::GameOperationController, image_controller::ImageController,
    request_controller::RequestController, ApiOperation, AssetCommand,
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
    char_pack_summaries: RwLock<Option<CharPackSummaryTable>>,
    refreshing: AtomicBool,

    api_model: Arc<ApiModel>,
    app_state_controller: Arc<AppStateController>,
    request_controller: Arc<RequestController>,
    image_controller: Arc<ImageController>,
    game_operation_controller: Arc<GameOperationController>,
}

impl GameController {
    pub fn new(
        api_model: Arc<ApiModel>,
        app_state_controller: Arc<AppStateController>,
        request_controller: Arc<RequestController>,
        image_controller: Arc<ImageController>,
        game_operation_controller: Arc<GameOperationController>,
    ) -> Self {
        Self {
            game_resource_map: RwLock::new(HashMap::new()),
            stage_data: RwLock::new(None),
            char_pack_summaries: RwLock::new(None),
            refreshing: AtomicBool::new(false),

            api_model,
            app_state_controller,
            request_controller,
            image_controller,
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
            .request_controller
            .send_api_request(ApiOperation::RetrieveGames { resp }, &mut rx)
            .await
        {
            Ok(_) => {
                self.try_fetch_all_game_details().await;
                self.process_game_list_changes(refresh_log_cond).await;
                self.app_state_controller
                    .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetched));
            }
            Err(e) => {
                println!("[Controller] Error retrieving games {}", e);
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
        self.request_controller
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

                                self.api_model.user.handle_retrieve_games_result(games).await;
                                self.try_fetch_all_game_details().await;
                                self.process_game_list_changes(RefreshLogsCondition::Never).await;

                                if is_initial {
                                    is_initial = false;
                                    self.app_state_controller
                                        .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetched));
                                }
                            },
                            GameSseEvent::Close => {
                                println!("[Controller] Games SSE connection closed on occupied");
                                self.app_state_controller
                                    .exec(|x| x.set_sse_connect_state(SseConnectState::DisconnectedOccupiedElsewhere));
                                break;
                            },
                            GameSseEvent::Unrecognized(ev_type) => {
                                println!("[Controller] Unrecognized SSE event: {ev_type}");
                            }
                        },
                        Ok(None) => { /* 处理None? */ },
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

    pub async fn try_fetch_all_game_details(&self) {
        let mut games_to_fetch_details: Vec<String> = Vec::new();
        for game_ref in self.api_model.get_game_map_read().await.values() {
            let game = game_ref.game.read().await;
            if game.info.status.code == api_arkhost::GameStatus::Running {
                games_to_fetch_details.push(game.info.status.account.clone());
            }
        }
        let tasks = games_to_fetch_details
            .into_iter()
            .map(|account| async move {
                let (resp, mut rx) = oneshot::channel();
                let result = self
                    .request_controller
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
        let mut game_list: Vec<(i32, String, GameInfoModel)> = Vec::new();
        let game_map = self.api_model.get_game_map_read().await;

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
                GameInfoModel::from(&game),
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
            .exec(|x| x.update_game_views(game_list, false));

        join_all(load_images_task).await;
        let mut refresh_log_tasks = vec![];
        for id in game_ids {
            refresh_log_tasks
                .push(self.refresh_logs_if_needed(id.clone(), refresh_log_cond.clone()));
            self.try_apply_game_images(id.clone()).await;
        }
        join_all(refresh_log_tasks).await;
    }

    pub async fn update_game_settings(&self, account: String, config_fields: GameConfigFields) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .request_controller
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
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
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
            .request_controller
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
                let model = GameInfoModel::from(game);
                self.app_state_controller
                    .exec(|x| x.update_game_view(id, Some(model), true));
            }
            Err(e) => {
                eprintln!("[Controller] error retrieving logs for game with id {id}: {e}");
                self.app_state_controller
                    .exec(|x| x.update_game_view(id, None, true));
            }
        }
    }

    pub async fn try_ensure_game_images(&self, game: &api_model::GameEntry, id: String) {
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
                        .map(|x| x.clone())
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
            .request_controller
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
            self.retrieve_logs(id.into(), GameLogLoadRequestType::Later)
                .await;
        }
    }
}
