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
use tokio::sync::{oneshot, Mutex, RwLock};

use std::{collections::HashMap, sync::Arc};

use super::{ApiCommand, AssetCommand};

enum CaptchaState {
    Running,
    Succeeded,
    Failed
}

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
    captcha_states: Mutex<HashMap<String, CaptchaState>>,
}

impl GameController {
    pub fn new() -> Self {
        Self {
            game_resource_map: RwLock::new(HashMap::new()),
            stage_data: RwLock::new(None),
            char_pack_summaries: RwLock::new(None),
            captcha_states: Mutex::new(HashMap::new()),
        }
    }

    pub async fn refresh_games(
        &self,
        parent: Arc<super::ControllerHub>,
        refresh_log_cond: super::RefreshLogsCondition,
    ) {
        self.load_resources(parent.clone()).await;
        let (resp, mut rx) = oneshot::channel();
        parent
            .get_app_state()
            .set_fetch_games_state(FetchGamesState::Fetching);
        match parent
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
                    let result = parent
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
                        let mut captcha_states = self.captcha_states.lock().await;
                        match captcha_states.get(&game_info.info.status.account) {
                            None => {
                                let parent = parent.clone();
                                let account = game_info.info.status.account.clone();
                                let gt = game_info.info.captcha_info.gt.clone();
                                let challenge = game_info.info.captcha_info.challenge.clone();
                                tokio::task::spawn(async move {
                                    let result = parent
                                        .game_operation_controller
                                        .preform_game_captcha(
                                            parent.clone(),
                                            account.clone(),
                                            gt,
                                            challenge,
                                        )
                                        .await;
                                    parent.game_controller.captcha_states.lock().await.insert(
                                        account,
                                        if result.is_ok() {
                                            CaptchaState::Succeeded
                                        } else {
                                            CaptchaState::Failed
                                        },
                                    );
                                });
                                captcha_states.insert(
                                    game_info.info.status.account.clone(),
                                    CaptchaState::Running,
                                );
                            }
                            _ => {}
                        }
                    }

                    self.load_game_images_if_empty(
                        parent.clone(),
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
                let app_state = parent.get_app_state();
                app_state.update_game_views(game_list, false);
                app_state.set_fetch_games_state(FetchGamesState::Fetched);

                for id in game_ids {
                    {
                        let parent = parent.clone();
                        let id = id.clone();
                        let refresh_log_cond = refresh_log_cond.clone();
                        tokio::task::spawn(async move {
                            parent
                                .game_controller
                                .refresh_logs_if_needed(parent.clone(), id.into(), refresh_log_cond)
                                .await
                        });
                    }
                    {
                        let parent = parent.clone();
                        tokio::task::spawn(async move {
                            parent
                                .game_controller
                                .apply_game_images_if_exist(parent.clone(), id.clone())
                                .await;
                        });
                    }
                }
            }
            Err(e) => {
                println!("[Controller] Error retrieving games {}", e);
                parent
                    .get_app_state()
                    .set_fetch_games_state(FetchGamesState::Retrying);
            }
        };
    }

    pub async fn update_game_settings(
        &self,
        parent: Arc<super::ControllerHub>,
        account: String,
        config_fields: GameConfigFields,
    ) {
        let (resp, mut rx) = oneshot::channel();
        match parent
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
            Ok(_) => {
                self.refresh_games(parent.clone(), super::RefreshLogsCondition::Never)
                    .await
            }
            Err(e) => eprintln!("[Controller] Error stopping game {e}"),
        }

        parent
            .get_app_state()
            .set_game_save_state(account.clone(), GameOptionSaveState::Idle);
    }

    pub async fn retrieve_logs(
        &self,
        parent: Arc<super::ControllerHub>,
        id: String,
        load_spec: GameLogLoadRequestType,
    ) {
        let spec = match load_spec {
            GameLogLoadRequestType::Later => RetrieveLogSpec::Latest {},
            GameLogLoadRequestType::Former => RetrieveLogSpec::Former {},
        };
        let (resp, mut rx) = oneshot::channel();
        match parent
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
                parent
                    .get_app_state()
                    .update_game_view(id, Some(model), true);
            }
            Err(e) => {
                eprintln!("[Controller] error retrieving logs for game with id {id}: {e}");
                parent.get_app_state().update_game_view(id, None, true);
            }
        }
    }

    pub async fn load_game_images_if_empty(
        &self,
        parent: Arc<super::ControllerHub>,
        info: &api_controller::GameInfo,
        id: String,
    ) {
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
        let image_controller = &parent.image_controller;
        image_controller
            .load_game_avatar_if_empty(parent.clone(), info, game_resource_entry.avatar.clone())
            .await;
        image_controller
            .load_game_char_illust_if_empty(
                parent.clone(),
                info,
                game_resource_entry.char_illust.clone(),
            )
            .await;

        if let Some(details) = &info.details {
            if !details.status.secretary_skin_id.is_empty() {
                _ = game_resource_entry
                    .char_illust_filename
                    .write()
                    .await
                    .insert(details.status.get_secretary_skin_id_escaped());
            }
        }
    }

    pub async fn apply_game_images_if_exist(&self, parent: Arc<super::ControllerHub>, id: String) {
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
                parent
                    .get_app_state()
                    .set_game_images(id, avatar_image_data, char_illust_data);
            }
        }
    }

    pub async fn load_resources(&self, parent: Arc<super::ControllerHub>) {
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
                .load_json_table::<CharPackSummaryTable>(parent, path.clone(), Some(path))
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

    async fn load_json_table<T>(
        &self,
        parent: Arc<super::ControllerHub>,
        path: String,
        cache_key: Option<String>,
    ) -> Option<T>
    where
        T: 'static + Send + Sync + for<'de> Deserialize<'de>,
    {
        let (resp, mut rx) = oneshot::channel();
        match parent
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

    async fn refresh_logs_if_needed(
        &self,
        parent: Arc<super::ControllerHub>,
        id: String,
        cond: super::RefreshLogsCondition,
    ) {
        parent.clone().get_app_state().get_game_by_id(
            id.clone(),
            move |game_info_list, i, mut game_info| {
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
                    let load_spec = GameLogLoadRequestType::Later;
                    let parent = parent.clone();
                    tokio::spawn(async move {
                        parent
                            .game_controller
                            .retrieve_logs(parent.clone(), id.into(), load_spec)
                            .await
                    });
                }
            },
        )
    }
}
