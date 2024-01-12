use crate::app::{
    api_controller::{self, RetrieveLogSpec},
    app_state::model::{CharIllust, GameInfoModel, ImageDataRaw, ImageDataRef},
    asset_controller::AssetRef,
    game_data::{CharPack, CharPackSummaryTable, StageTable},
    ui::*,
};
use anyhow::anyhow;
use arkhost_api::models::api_arkhost::{self, GameConfigFields, GameStatus};
use serde::Deserialize;
use slint::Model;
use tokio::sync::{oneshot, RwLock};

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{
    app_state_controller::AppStateController, game_operation_controller::GameOperationController,
    image_controller::ImageController, request_controller::RequestController, ApiCommand,
    AssetCommand,
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

    app_state_controller: Arc<AppStateController>,
    request_controller: Arc<RequestController>,
    image_controller: Arc<ImageController>,
    game_operation_controller: Arc<GameOperationController>,
}

impl GameController {
    pub fn new(
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

        self.load_resources().await;
        let (resp, mut rx) = oneshot::channel();
        self.app_state_controller
            .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetching));
        match self
            .request_controller
            .send_api_request(ApiCommand::RetrieveGames { resp }, &mut rx)
            .await
        {
            Ok(games) => {
                let mut games_to_fetch_details: Vec<String> = Vec::new();
                for game_ref in games.read().await.values() {
                    let game_info = game_ref.info.read().await;
                    if game_info.info.status.code == api_arkhost::GameStatus::Running {
                        games_to_fetch_details.push(game_info.info.status.account.clone());
                    }
                }
                for account in games_to_fetch_details {
                    let (resp, mut rx) = oneshot::channel();
                    let result = self
                        .request_controller
                        .send_api_request(
                            ApiCommand::RetrieveGameDetails {
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
                }

                let mut game_list: Vec<(i32, String, GameInfoModel)> = Vec::new();
                for game_ref in games.read().await.values() {
                    let game_info = game_ref.info.read().await;
                    if game_info.info.status.code == GameStatus::Captcha {
                        let account = game_info.info.status.account.clone();
                        let gt = game_info.info.captcha_info.gt.clone();
                        let challenge = game_info.info.captcha_info.challenge.clone();
                        self.game_operation_controller
                            .preform_game_captcha(account.clone(), gt, challenge)
                            .await;
                    }

                    self.load_game_images_if_empty(
                        &game_info,
                        game_info.info.status.account.clone(),
                    )
                    .await;
                    game_list.push((
                        game_ref.order,
                        game_info.info.status.account.clone(),
                        GameInfoModel::from(&game_info),
                    ));
                }
                game_list.sort_by_key(|(order, _, _)| *order);

                let game_ids: Vec<String> = game_list.iter().map(|(_, id, _)| id.clone()).collect();
                self.app_state_controller
                    .exec(|x| x.update_game_views(game_list, false));
                self.app_state_controller
                    .exec(|x| x.set_fetch_games_state(FetchGamesState::Fetched));

                for id in game_ids {
                    self.refresh_logs_if_needed(id.clone(), refresh_log_cond.clone())
                        .await;
                    self.apply_game_images_if_exist(id.clone()).await;
                }
            }
            Err(e) => {
                println!("[Controller] Error retrieving games {}", e);
                self.app_state_controller
                    .exec(|x| x.set_fetch_games_state(FetchGamesState::Retrying));
            }
        };

        self.refreshing.store(false, Ordering::Release);
    }

    pub async fn update_game_settings(&self, account: String, config_fields: GameConfigFields) {
        let (resp, mut rx) = oneshot::channel();
        match self
            .request_controller
            .send_api_request(
                ApiCommand::UpdateGameSettings {
                    account: account.clone(),
                    config: config_fields,
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(_) => self.refresh_games(super::RefreshLogsCondition::Never).await,
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
                ApiCommand::RetrieveLog {
                    account: id.clone(),
                    spec,
                    resp,
                },
                &mut rx,
            )
            .await
        {
            Ok(game_ref) => {
                let game = &game_ref.info.read().await;
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

    pub async fn load_game_images_if_empty(&self, info: &api_controller::GameInfo, id: String) {
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
        image_controller
            .load_game_avatar_if_empty(info, game_resource_entry.avatar.clone())
            .await;
        image_controller
            .load_game_char_illust_if_empty(info, game_resource_entry.char_illust.clone())
            .await;

        if let Some(details) = &info.details {
            if !details.status.secretary_skin_id.is_empty() {
                _ = game_resource_entry
                    .char_illust_filename
                    .write()
                    .await
                    .insert(details.status.sanitize_secretary_skin_id_for_url());
            }
        }
    }

    pub async fn apply_game_images_if_exist(&self, id: String) {
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
                    if let Some(char_pack) = self
                        .char_pack_summaries
                        .read()
                        .await
                        .as_ref()
                        .and_then(|x| x.get(char_illust_filename))
                        .map(|x| x.clone())
                        .or_else(|| {
                            Some(CharPack {
                                name: "".into(),
                                pivot_factor: [0.5, 0.5],
                                pivot_offset: [0., 0.],
                                scale: [0.7, 0.7], // 不确定pivot的情况下尽可能缩小图像，以免细节被遮挡
                                size: [100., 100.],
                            })
                        })
                    {
                        char_illust_data = Some(CharIllust {
                            image: image_data.clone(),
                            positions: char_pack.clone(),
                        })
                    }
                }
            }

            if avatar_image_data.is_some() || char_illust_data.is_some() {
                self.app_state_controller
                    .exec(|x| x.set_game_images(id, avatar_image_data, char_illust_data));
            }
        }
    }

    pub async fn load_resources(&self) {
        // let has_stage_data = self.stage_data.read().await.is_some();
        let has_char_pack_summary = self.char_pack_summaries.read().await.is_some();

        // if !has_stage_data {
        //     if let Some(stage_data) = self
        //         .load_json_table::<StageTable>(parent, "stages.json".into(), Some("stages.json".into()))
        //         .await
        //     {
        //         _ = self.stage_data.write().await.insert(stage_data);
        //     }
        // }

        if !has_char_pack_summary {
            let path = arkhost_api::consts::asset::api::charpack("summary.json");
            if let Some(char_pack_summary) = self
                .load_json_table::<CharPackSummaryTable>(path.clone(), Some(path))
                .await
            {
                _ = self
                    .char_pack_summaries
                    .write()
                    .await
                    .insert(char_pack_summary);
            }
        }
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
                            should_refresh_val.store(true, Ordering::Relaxed);
                        }
                    })
                })
                .await;
        }

        if should_refresh_val.load(Ordering::Relaxed) {
            self.retrieve_logs(id.into(), GameLogLoadRequestType::Later)
                .await;
        }
    }
}
