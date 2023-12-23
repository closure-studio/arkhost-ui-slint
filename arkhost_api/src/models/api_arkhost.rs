use bitflags::bitflags;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use serde_with::{serde_as, TimestampSeconds};

#[derive(Default, PartialEq, Deserialize_repr, Clone, Debug)]
#[repr(i32)]
pub enum GameStatus {
    LoginFailed = -1,
    #[default]
    Pending = 0,
    Logging = 1,
    Running = 2,
    Error = 3,
}

#[derive(Default, Deserialize, Clone, Debug)]
pub struct LogLevel(u32);
bitflags! {
    impl LogLevel: u32 {
        const FATAL = 1 << 7; // System
        const NOTICE = 1 << 6; // User side
        const ERROR = 1 << 5; // System
        const WARNING = 1 << 4; // System
        const SYSTEM = 1 << 3; // System
        const COMMON = 1 << 2; // System
        const HELP = 1 << 1; // System
        const DEBUG = 1 << 0; // System

        const DEFAULT = !0;
    }
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct GameInfo {
    pub status: Status,
    pub captcha_info: CaptchaInfo,
    pub game_config: GameConfigFields,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Status {
    pub account: String,
    pub platform: u32,
    pub uuid: String,
    pub code: GameStatus,
    pub text: String,
    pub nick_name: String,
    pub level: u32,
    pub avatar: Avatar,
    pub created_at: u64,
    pub is_verify: bool,
    pub ap: u32,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GameDetails {
    pub status: StatusDetails,
    pub config: GameConfigFields,
    pub screenshot: Option<Vec<Screenshots>>,
}

#[serde_as]
#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StatusDetails {
    pub android_diamond: u32,
    pub ap: u32,
    pub avatar: Avatar,
    pub avatar_id: String,
    pub diamond_shard: u32,
    pub gacha_ticket: u32,
    pub ten_gacha_ticket: u32,
    pub gold: u32,
    #[serde_as(as = "TimestampSeconds<i64>")]
    pub last_ap_add_time: DateTime<Utc>,
    pub level: u32,
    pub max_ap: u32,
    pub nick_name: String,
    pub recruit_license: u32,
    pub secretary: String,
    pub secretary_skin_id: String,
    pub social_point: u32,
}

impl StatusDetails {
    pub fn get_secretary_skin_id_escaped(&self) -> String {
        self.secretary_skin_id.replace("@", "_").replace("#", "_")
    }
}

#[serde_as]
#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Screenshots {
    #[serde(rename = "UTCTime")]
    #[serde_as(as = "TimestampSeconds<i64>")]
    pub utc_time: DateTime<Utc>,
    pub file_name: Vec<String>,
    pub host: String,
    pub url: String,
    #[serde(rename = "type")]
    pub type_val: u32,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Avatar {
    #[serde(rename = "type")]
    pub type_val: String,
    pub id: String,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct CaptchaInfo {
    pub captcha_type: String,
    pub challenge: String,
    pub created: i32,
    pub gt: String,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct GameConfigFields {
    pub is_auto_battle: Option<bool>,
    pub is_stopped: Option<bool>,
    pub keeping_ap: Option<i32>,
    pub map_id: Option<String>,
    pub battle_maps: Option<Vec<String>>,
    pub recruit_ignore_robot: Option<bool>,
    pub recruit_reserve: Option<i32>,
    pub enable_building_arrange: Option<bool>,
}

impl GameConfigFields {
    pub fn new() -> Self {
        Self {
            is_auto_battle: None,
            is_stopped: None,
            keeping_ap: None,
            map_id: None,
            battle_maps: None,
            recruit_ignore_robot: None,
            recruit_reserve: None,
            enable_building_arrange: None,
        }
    }
}

#[derive(Default, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct UpdateGameRequest {
    pub config: GameConfigFields,
}

#[serde_as]
#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    #[serde_as(as = "TimestampSeconds<i64>")]
    pub ts: DateTime<Utc>,
    pub log_level: LogLevel,
    pub content: String,
}

impl LogEntry {
    pub fn local_ts(&self) -> DateTime<Local> {
        self.ts.with_timezone(&Local)
    }
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetLogResponse {
    pub logs: Vec<LogEntry>,
    pub has_more: bool,
}
