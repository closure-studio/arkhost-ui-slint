use std::rc::Rc;

use crate::app::api_user_model::GameEntry;
use crate::app::api_user_model::{SlotEntry, SlotSyncState};
use crate::app::ui::*;
use arkhost_api::models::api_arkhost::{self, GameConfigFields, GamePlatform};
use arkhost_api::models::api_passport;
use arkhost_api::models::api_quota::{user_tier_availability_rank, UpdateSlotAccountRequest};
use slint::{ModelRc, SharedString, VecModel};

pub struct GameInfoMapping {
    pub game: GameEntry,
}

impl GameInfoMapping {
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
        game_info.battle_map = match self.game.stage_name.as_ref().or(self
            .game
            .info
            .game_config
            .map_id
            .as_ref())
        {
            Some(map) if !map.is_empty() => map,
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
            nickname => format!("Dr. {nickname}"),
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
            let log_mapping =
                GameLogMapping::from(self.game.logs.as_slices(), self.game.log_cursor_back);
            log_mapping.mutate(game_info);
        }
        let options_mapping = GameOptionsMapping::from(&self.game.info.game_config);
        options_mapping.mutate(&mut game_info.options);
        if self.game.info.status.code == api_arkhost::GameStatus::Running {
            if let Some(ref game_details) = self.game.details {
                let details_mapping = GameDetailsMapping::from(game_details.status.clone());
                details_mapping.mutate(&mut game_info.details);
            }
        }
    }
}

pub struct GameDetailsMapping {
    details: api_arkhost::StatusDetails,
}

impl GameDetailsMapping {
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

pub struct GameLogMapping<'a> {
    logs: (&'a [api_arkhost::LogEntry], &'a [api_arkhost::LogEntry]),
    log_cursor: u64,
}

impl<'a> GameLogMapping<'a> {
    pub fn from(
        logs: (&'a [api_arkhost::LogEntry], &'a [api_arkhost::LogEntry]),
        log_cursor: u64,
    ) -> Self {
        Self { logs, log_cursor }
    }

    pub fn mutate(&self, game_info: &mut GameInfo) {
        game_info.log_loaded = match self.log_cursor {
            _ if game_info.log_loaded == GameLogLoadState::Loading => GameLogLoadState::Loading,
            0 => GameLogLoadState::NotLoaded,
            _ => GameLogLoadState::Loaded,
        };

        let it = self.logs.0.iter().chain(self.logs.1.iter());
        let logs: Vec<GameLogEntry> = it
            .map(|x| {
                let attributes = if x.log_level.bits() == api_arkhost::LogLevel::NOTICE.bits() {
                    SharedString::new()
                } else {
                    x.log_level.attributes_tag().into()
                };

                let mut str = x.content.to_string();
                str.push(' '); // bug: 在开启word-wrap时，字符串尾部是中文标点会导致错误换行
                GameLogEntry {
                    timestamp: x.local_ts().format("%m-%d.%H:%M:%S").to_string().into(),
                    content: str.into(),
                    attributes,
                }
            })
            .collect();
        game_info.logs = ModelRc::from(Rc::new(VecModel::from(logs)));
    }
}

pub struct GameOptionsMapping {
    options: api_arkhost::GameConfigFields,
}

impl GameOptionsMapping {
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
                accelerate_slot_cn: Some(options.accelerate_slot_cn.to_string()),
                is_stopped: None,
                map_id: None,
            },
        }
    }

    pub fn to_game_options(&self) -> GameConfigFields {
        self.options.clone()
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
        game_options.accelerate_slot_cn = self
            .options
            .accelerate_slot_cn
            .as_deref()
            .unwrap_or("中层左")
            .into();
    }
}

#[derive(Debug, Clone)]
pub struct SlotInfoMapping {
    pub slot_entry: SlotEntry,
}

impl SlotInfoMapping {
    pub fn from(slot_entry: SlotEntry) -> Self {
        Self { slot_entry }
    }

    pub fn mutate(&self, slot_info: &mut SlotInfo) {
        slot_info.uuid = self.slot_entry.data.uuid.clone().into();
        slot_info.description = self.slot_description().into();
        slot_info.game_account = self.game_account_without_prefix().into();
        slot_info.game_account_split = Rc::new(self.try_split_email()).into();
        slot_info.platform = self.slot_platform();
        slot_info.state = match self.slot_entry.sync_state {
            _ if self.slot_entry.data.game_account.is_none() => SlotState::Empty,
            SlotSyncState::Unknown => SlotState::UnknownSyncState,
            SlotSyncState::Synchronized => SlotState::Synchronized,
            SlotSyncState::Pending => SlotState::HasPendingUpdate,
        };

        if let Some(last_update_request) = &self.slot_entry.last_update_request {
            slot_info.last_verify =
                SlotUpdateDraftMapping::from(last_update_request).to_update_draft();
        }

        let mut requirements = vec![];
        for rule_flag in &self.slot_entry.data.rule_flags {
            requirements.push((
                rule_flag.clone(),
                SlotRequirement {
                    description: rule_flag.default_description().into(),
                    ..SlotRequirement::default()
                },
            ));
        }

        if let Some(verify_result) = self.slot_entry.last_update_response.as_ref() {
            for (flag, result) in &verify_result.results {
                if let Some((_, requirement)) = requirements.iter_mut().find(|(x, _)| *x == *flag) {
                    requirement.has_result = true;
                    requirement.fulfilled = result.available;
                    requirement.status_text = result.message.clone().into();
                } else {
                    requirements.push((
                        flag.clone(),
                        SlotRequirement {
                            description: flag.default_description().into(),
                            has_result: true,
                            fulfilled: result.available,
                            status_text: result.message.clone().into(),
                        },
                    ));
                }
            }
        }

        let mut requirements: Vec<SlotRequirement> =
            requirements.into_iter().map(|(_, x)| x).collect();
        if requirements.is_empty() {
            requirements.push(SlotRequirement {
                description: "无".into(),
                ..Default::default()
            });
        }
        slot_info.verify_rules = Rc::new(VecModel::from(requirements)).into();
    }

    pub fn create_slot_info(&self) -> SlotInfo {
        let mut slot_info = SlotInfo {
            view_state: SlotDetailsViewState::Collapsed,
            ..Default::default()
        };
        self.mutate(&mut slot_info);
        slot_info
    }

    fn slot_description(&self) -> String {
        match self.slot_entry.data.user_tier_availability_rank() {
            user_tier_availability_rank::TIER_BASIC => "可露希尔托管凭证 · 基础型",
            user_tier_availability_rank::TIER_SMS_VERIFIED => "可露希尔托管凭证 · 改良型",
            user_tier_availability_rank::TIER_QQ_VERIFIED => "可露希尔托管凭证 · 超级改",
            _ => "可露希尔托管凭证 · 特制款",
        }
        .into()
    }

    fn slot_platform(&self) -> SlotPlatform {
        match self
            .slot_entry
            .data
            .game_account
            .as_ref()
            .and_then(|x| x.chars().next())
        {
            Some('G') => SlotPlatform::Official,
            Some('B') => SlotPlatform::Bilibili,
            _ => SlotPlatform::None,
        }
    }

    fn game_account_without_prefix(&self) -> String {
        if let Some(game_account) = &self.slot_entry.data.game_account {
            game_account[1..].to_owned()
        } else {
            String::default()
        }
    }

    fn try_split_email(&self) -> VecModel<SharedString> {
        if let Some(game_account) = &self.slot_entry.data.game_account {
            let mut iter = game_account[1..].splitn(2, '@');
            if let (Some(s1), Some(s2)) = (iter.next(), iter.next()) {
                return vec![s1.into(), ("@".to_owned() + s2).into()].into();
            }
        }

        if let Some(game_account) = &self.slot_entry.data.game_account {
            vec![game_account[1..].to_owned().into(), SharedString::default()].into()
        } else {
            vec![SharedString::default(), SharedString::default()].into()
        }
    }
}

#[derive(Debug, Clone)]
pub struct SlotUpdateDraftMapping {
    pub update_request: UpdateSlotAccountRequest,
}

impl SlotUpdateDraftMapping {
    pub fn from(update_request: &UpdateSlotAccountRequest) -> Self {
        Self {
            update_request: update_request.clone(),
        }
    }

    pub fn from_ui(update_draft: &SlotUpdateDraft) -> Option<UpdateSlotAccountRequest> {
        match update_draft.update_type {
            SlotUpdateDraftType::Unchanged => None,
            SlotUpdateDraftType::Update => match update_draft.platform {
                SlotPlatform::None => None,
                SlotPlatform::Official => Some(GamePlatform::Official),
                SlotPlatform::Bilibili => Some(GamePlatform::Bilibili),
            }
            .map(|platform| UpdateSlotAccountRequest::SaveAccount {
                platform,
                account: update_draft.game_account.trim().to_string(),
                password: update_draft.password.to_string(),
            }),
            SlotUpdateDraftType::Delete => {
                Some(UpdateSlotAccountRequest::ClearAccount { account: () })
            }
        }
    }

    pub fn to_update_draft(&self) -> SlotUpdateDraft {
        let mut update_draft = SlotUpdateDraft::default();
        self.mutate(&mut update_draft);
        update_draft
    }

    pub fn mutate(&self, update_draft: &mut SlotUpdateDraft) {
        match &self.update_request {
            UpdateSlotAccountRequest::SaveAccount {
                account,
                platform,
                password,
            } => {
                update_draft.update_type = SlotUpdateDraftType::Update;
                update_draft.platform = match platform {
                    GamePlatform::Official => SlotPlatform::Official,
                    GamePlatform::Bilibili => SlotPlatform::Bilibili,
                };
                update_draft.game_account = account.clone().into();
                update_draft.password = password.clone().into();
            }
            UpdateSlotAccountRequest::ClearAccount { .. } => {
                update_draft.update_type = SlotUpdateDraftType::Delete;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserInfoMapping {
    pub uuid: String,
    pub nickname: String,
    pub status: api_passport::UserStatus,
    pub is_admin: bool,
    pub phone: String,
    pub qq: String,
    pub sms_verify_slot_added: bool,
}

impl UserInfoMapping {
    pub fn mutate(&self, user_info: &mut UserInfo) {
        user_info.uuid = self.uuid.clone().into();
        user_info.nickname = self.nickname.clone().into();
        user_info.status = match self.status {
            api_passport::UserStatus::SmsUnverified => UserStatus::SmsUnverified,
            api_passport::UserStatus::Normal => UserStatus::Normal,
            api_passport::UserStatus::Banned => UserStatus::Banned,
            api_passport::UserStatus::ManuallyVerified => UserStatus::ManuallyVerified,
            api_passport::UserStatus::UnsupportedStatus => UserStatus::Normal,
        };
        user_info.tier = match self.status {
            api_passport::UserStatus::SmsUnverified | api_passport::UserStatus::Normal
                if self.phone.is_empty() =>
            {
                UserTier::Basic
            }
            _ if self.qq.is_empty() => UserTier::SmsVerified,
            _ if !self.qq.is_empty() => UserTier::QQVerified,
            _ => UserTier::Invalid,
        };
        user_info.progress = match user_info.tier {
            UserTier::Invalid => UserProgress::Invalid,
            UserTier::Basic if !self.sms_verify_slot_added => UserProgress::Initial,
            UserTier::Basic => UserProgress::SmsVerifySlotAdded,
            UserTier::SmsVerified => UserProgress::SmsVerified,
            UserTier::QQVerified => UserProgress::QQVerified,
        };

        user_info.phone = self.phone.clone().into();
        user_info.qq = self.qq.clone().into();
        user_info.is_admin = self.is_admin;
    }
}

#[derive(Debug, Clone)]
pub struct BattleMapMapping {
    pub map_id: String,
    pub code_name: String,
    pub display_name: String,
}

impl BattleMapMapping {
    pub fn create_battle_map(&self) -> BattleMap {
        let mut battle_map = BattleMap::default();
        self.mutate(&mut battle_map);
        battle_map
    }

    pub fn mutate(&self, battle_map: &mut BattleMap) {
        battle_map.map_id = self.map_id.clone().into();
        battle_map.code_name = self.code_name.clone().into();
        battle_map.display_name = self.display_name.clone().into();
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
        result
    }
}
