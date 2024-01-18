use std::rc::Rc;

use crate::app::ui::*;
use crate::app::{api_model::GameEntry, game_data};
use arkhost_api::models::api_arkhost::{self, GameConfigFields};
use slint::{ModelRc, SharedString, VecModel};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GameInfoModel {
    pub game: GameEntry,
}

/**
GameInfo {
    ap: (),
    battle_map: (),
    doctor_level: (),
    doctor_name: (),
    doctor_serial: (),
    game_state: (),
    id: (),
    log_loaded: (),
    logs: (),
    options: (),
    request_state: (),
    save_state: (),
}
*/
impl GameInfoModel {
    pub fn from(game: &GameEntry) -> Self {
        Self { game: game.clone() }
    }

    pub fn create_game_info(&self) -> GameInfo {
        let mut game_info = GameInfo::default();
        self.mutate(&mut game_info, true);
        game_info.request_state = GameOperationRequestState::Idle;
        game_info.save_state = GameOptionSaveState::Idle;
        game_info
    }

    pub fn mutate(&self, game_info: &mut GameInfo, refresh_logs: bool) {
        game_info.ap = self.game.info.status.ap.to_string().into();
        game_info.battle_map = match self.game.info.game_config.map_id {
            Some(ref map) if map != "" => map,
            _ => "[作战未开始]",
        }
        .into();
        game_info.doctor_level = match self.game.info.status.level {
            0 => "-".to_string(),
            val => val.to_string(),
        }
        .into();
        game_info.doctor_name = match self.game.info.status.nick_name.as_str() {
            "" => "未登录".to_string(),
            nickname => format!("Dr. {}", nickname),
        }
        .into();
        // 未实现玩家编号#1234，使用账号代替
        // 需要码掉手机号“G199------88”，mask为"-"
        // 邮箱能码但是不完全能码
        game_info.doctor_serial = utils::redact_account(&self.game.info.status.account).into();
        game_info.game_state = match self.game.info.status.code {
            api_arkhost::GameStatus::Captcha => GameState::Captcha,
            api_arkhost::GameStatus::LoginFailed => GameState::Faulted,
            api_arkhost::GameStatus::Pending => GameState::Stopped,
            api_arkhost::GameStatus::Logging => GameState::Logging,
            api_arkhost::GameStatus::Running => GameState::Running,
            api_arkhost::GameStatus::Error
            | api_arkhost::GameStatus::ErrorLoggedOut
            | api_arkhost::GameStatus::ErrorBattleFailed
            | api_arkhost::GameStatus::ErrorCaptchaTimedOut => GameState::Faulted,
        };
        if game_info.game_state == GameState::Faulted {
            game_info.status_text = self.game.info.status.text.clone().into();
        }

        game_info.id = self.game.info.status.account.clone().into();
        if refresh_logs {
            let log_model = GameLogModel::from(&self.game.logs, self.game.log_cursor_back);
            log_model.mutate(game_info);
        }
        let options_model = GameOptionsModel::from(&self.game.info.game_config);
        options_model.mutate(&mut game_info.options);
        if self.game.info.status.code == api_arkhost::GameStatus::Running {
            if let Some(ref game_details) = self.game.details {
                let details_model = GameDetailsModel::from(game_details.status.clone());
                details_model.mutate(&mut game_info.details);
            }
        }
    }
}

pub struct GameDetailsModel {
    details: api_arkhost::StatusDetails,
}

impl GameDetailsModel {
    pub fn from(details: api_arkhost::StatusDetails) -> Self {
        Self { details }
    }

    pub fn mutate(&self, game_details: &mut GameDetails) {
        game_details.loaded = true;
        game_details.diamond = self.details.android_diamond.to_string().into();
        game_details.diamond_shard = self.details.diamond_shard.to_string().into();
        game_details.gacha_ticket = self.details.gacha_ticket.to_string().into();
        game_details.tenfold_gacha_ticket = self.details.ten_gacha_ticket.to_string().into();
        game_details.gold = self.details.gold.to_string().into();
        game_details.max_ap = self.details.max_ap.to_string().into();
        game_details.recruit_license = self.details.recruit_license.to_string().into();
        game_details.social_point = self.details.social_point.to_string().into();
    }
}

pub struct GameLogModel {
    logs: Vec<api_arkhost::LogEntry>,
    log_cursor: u64,
}

impl GameLogModel {
    pub fn from(logs: &Vec<api_arkhost::LogEntry>, log_cursor: u64) -> Self {
        Self {
            logs: logs.clone(),
            log_cursor,
        }
    }

    pub fn mutate(&self, game_info: &mut GameInfo) {
        game_info.log_loaded = match self.log_cursor {
            _ if game_info.log_loaded == GameLogLoadState::Loading => GameLogLoadState::Loading,
            0 => GameLogLoadState::NotLoaded,
            _ => GameLogLoadState::Loaded,
        };
        let logs: Vec<GameLogEntry> = self
            .logs
            .iter()
            .map(|x| {
                let mut str = x.content.to_string();
                str.push(' '); // bug: 在开启word-wrap时，字符串尾部是中文标点会导致错误换行
                GameLogEntry {
                    timestamp: x.local_ts().format("%m-%d.%H:%M:%S").to_string().into(),
                    content: str.into(),
                }
            })
            .collect();

        game_info.logs = ModelRc::from(Rc::new(VecModel::from(logs)));
    }
}

pub struct GameOptionsModel {
    options: api_arkhost::GameConfigFields,
}

/**
GameOptions {
    ap_reserve: (),
    battle_maps: (),
    enable_auto_battle: (),
    enable_building_arrange: (),
    recruit_ignore_robot: (),
    recruit_reserve: (),
}
*/

impl GameOptionsModel {
    pub fn from(options: &api_arkhost::GameConfigFields) -> Self {
        Self {
            options: options.clone(),
        }
    }

    pub fn from_ui(options: &GameOptions) -> Self {
        Self {
            options: GameConfigFields {
                keeping_ap: Some(options.ap_reserve),
                battle_maps: None,
                is_auto_battle: Some(options.enable_auto_battle),
                enable_building_arrange: Some(options.enable_building_arrange),
                recruit_ignore_robot: Some(options.recruit_ignore_robot),
                recruit_reserve: Some(options.recruit_reserve),
                is_stopped: None,
                map_id: None,
            },
        }
    }

    pub fn to_game_options(&self) -> GameConfigFields {
        return self.options.clone();
    }

    pub fn mutate(&self, game_options: &mut GameOptions) {
        game_options.ap_reserve = self.options.keeping_ap.unwrap_or(0);
        let battle_maps: Vec<SharedString> = self
            .options
            .battle_maps
            .as_ref()
            .map_or(vec![], |x| x.clone())
            .iter()
            .map(|x| x.into())
            .collect();
        game_options.battle_maps = ModelRc::from(Rc::new(VecModel::from(battle_maps)));
        game_options.enable_auto_battle = self.options.is_auto_battle.unwrap_or(true);
        game_options.enable_building_arrange = self.options.enable_building_arrange.unwrap_or(true);
        game_options.recruit_ignore_robot = self.options.recruit_ignore_robot.unwrap_or(false);
        game_options.recruit_reserve = self.options.recruit_reserve.unwrap_or(0);
    }
}

mod utils {
    pub fn redact_account(account: &str) -> String {
        const MASK: char = '-';
        const PREFIX_LEN: usize = 1; // [GB]前缀
        const MASK_LEN: usize = 6;
        let account_len = account.len();
        if account_len < PREFIX_LEN + MASK_LEN + 2 {
            // 最少剩前缀+前后两位
            return account.into();
        }

        let mut result = String::new();
        let start = std::cmp::min(
            PREFIX_LEN + 4 - 1,
            account_len - PREFIX_LEN - MASK_LEN + 1 - 1,
        );
        let end = start + MASK_LEN - 1;
        result.reserve(account_len * 2);
        for (i, ch) in account.chars().enumerate() {
            if i >= start && i <= end {
                result.push(MASK);
            } else {
                result.push(ch);
            }
        }
        return result;
    }
}

#[derive(Default, Debug, Clone)]
pub struct CharIllust {
    pub image: ImageData,
    pub positions: game_data::CharPack,
}

#[derive(Default, Debug, Clone)]
pub enum ImageDataRaw {
    #[default]
    Empty,
    Pending,
    Rgba8 {
        raw: bytes::Bytes,
        width: u32,
        height: u32,
    },
}

#[derive(Default, Debug, Clone)]
pub struct ImageData {
    pub asset_path: String,
    pub cache_key: Option<String>,
    pub format: Option<image::ImageFormat>,
    pub loaded_image: ImageDataRaw,
}

impl ImageData {
    pub fn to_slint_image(&self) -> Option<slint::Image> {
        match &self.loaded_image {
            ImageDataRaw::Rgba8 { raw, width, height } => Some(slint::Image::from_rgba8(
                slint::SharedPixelBuffer::clone_from_slice(raw, *width, *height),
            )),
            _ => None,
        }
    }
}

pub type ImageDataRef = Arc<RwLock<ImageData>>;
