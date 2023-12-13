use std::rc::Rc;

use crate::app::api_controller::GameInfo as ApiGameInfo;
use crate::app::ui::*;
use arkhost_api::models::api_arkhost::{self, GameConfigFields};
use slint::{ModelRc, SharedString, VecModel};

pub struct GameInfoRepresent {
    pub game_info: ApiGameInfo,
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
impl GameInfoRepresent {
    pub fn from(game_info: &ApiGameInfo) -> Self {
        Self {
            game_info: game_info.clone(),
        }
    }

    pub fn create_game_info(&self) -> GameInfo {
        let mut game_info = GameInfo::default();
        self.mutate(&mut game_info, true);
        game_info.log_loaded = GameLogLoadState::NotLoaded;
        game_info.request_state = GameOperationRequestState::Idle;
        game_info.save_state = GameOptionSaveState::Idle;
        game_info
    }

    pub fn mutate(&self, game_info: &mut GameInfo, refresh_logs: bool) {
        game_info.ap = self.game_info.info.status.ap.to_string().into();
        // TODO：从静态数据里面拿关卡名
        game_info.battle_map = self
            .game_info
            .info
            .game_config
            .map_id
            .as_ref()
            .map_or("无".to_string(), |x| x.clone())
            .into();
        game_info.doctor_level = match self.game_info.info.status.level {
            0 => "-".to_string(),
            val => val.to_string(),
        }
        .into();
        game_info.doctor_name = match self.game_info.info.status.nick_name.as_str() {
            "" => "未登录".to_string(),
            nickname => format!("Dr. {}", nickname),
        }
        .into();
        // 未实现玩家编号#1234，使用账号代替
        // 需要码掉手机号“G199····8888”，mask为中文标点"·"
        // 邮箱能码但是不完全能码
        game_info.doctor_serial = utils::mask_account(&self.game_info.info.status.account).into();
        game_info.game_state = match self.game_info.info.status.code {
            api_arkhost::GameStatus::Logging if self.game_info.info.captcha_info.created != 0 => {
                GameState::Captcha
            }
            api_arkhost::GameStatus::LoginFailed => GameState::Faulted,
            api_arkhost::GameStatus::Pending => GameState::Stopped,
            api_arkhost::GameStatus::Logging => GameState::Logging,
            api_arkhost::GameStatus::Running => GameState::Running,
            api_arkhost::GameStatus::Error => GameState::Faulted,
        };
        game_info.id = self.game_info.info.status.account.clone().into();
        if refresh_logs {
            let log_represent =
                GameLogRepresent::from(&self.game_info.logs, self.game_info.log_cursor_back);
            log_represent.mutate(game_info);
        }
        let options_represent = GameOptionsRepresent::from(&self.game_info.info.game_config);
        options_represent.mutate(&mut game_info.options);
    }
}

pub struct GameLogRepresent {
    logs: Vec<api_arkhost::LogEntry>,
    log_cursor: u64,
}

impl GameLogRepresent {
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

pub struct GameOptionsRepresent {
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

impl GameOptionsRepresent {
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
                map_id: None
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
    pub fn mask_account(account: &str) -> String {
        const MASK: char = '·';
        const PREFIX_LEN: usize = 1; // [GB]前缀
        const MASK_LEN: usize = 4;
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
