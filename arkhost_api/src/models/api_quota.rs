use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    api_arkhost::GamePlatform,
    api_passport::{UserPermissions, UserStatus},
    common::ResponseData,
};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub uuid: String,
    pub rule_flags: Vec<RuleFlag>,
    pub game_account: Option<String>,
}

impl Slot {
    pub fn user_tier_availability_rank(&self) -> i32 {
        self.rule_flags
            .iter()
            .fold(0, |acc, x| acc | x.user_tier_availability_rank())
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum UpdateSlotAccountRequest {
    SaveAccount {
        account: String,
        platform: GamePlatform,
        password: String,
    },
    ClearAccount {
        account: (),
    },
}

pub type UpdateSlotAccountResponse = ResponseData<SlotRuleValidationResult>;

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SlotRuleValidationResult {
    pub available: bool,
    pub results: HashMap<RuleFlag, RuleValidationResult>,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RuleValidationResult {
    pub available: bool,
    pub status_id: String,
    pub message: String,
}

#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub uuid: String,
    pub id_server_permission: UserPermissions,
    pub id_server_phone: String,
    #[serde(rename = "idServerQQ")]
    pub id_server_qq: String,
    pub id_server_status: UserStatus,
    pub slots: Vec<Slot>,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub enum RuleFlagId {
    #[serde(rename = "slot_account_sms_verified")]
    SlotAccountSmsVerified,
    #[serde(rename = "slot_user_sms_verified")]
    SlotUserSmsVerified,
    #[serde(rename = "slot_user_qq_verified")]
    SlotUserQQVerified,
    #[serde(rename = "slot_account_format_is_phone")]
    SlotAccountIsPhone,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[serde(untagged)]
pub enum RuleFlag {
    Id(RuleFlagId),
    Other(String),
}

// TODO: 以metadata方式获取
impl RuleFlag {
    pub fn user_tier_availability_rank(&self) -> i32 {
        match self {
            RuleFlag::Id(rule_flag_id) => match rule_flag_id {
                RuleFlagId::SlotAccountSmsVerified => user_tier_availability_rank::TIER_BASIC,
                RuleFlagId::SlotUserSmsVerified => user_tier_availability_rank::TIER_SMS_VERIFIED,
                RuleFlagId::SlotUserQQVerified => user_tier_availability_rank::TIER_QQ_VERIFIED,
                RuleFlagId::SlotAccountIsPhone => 0,
            },
            RuleFlag::Other(_) => 0,
        }
    }

    pub fn default_description(&self) -> String {
        match self {
            RuleFlag::Id(rule_flag_id) => match rule_flag_id {
                RuleFlagId::SlotAccountSmsVerified => "仅限归属认证用帐号（可接收验证短信）".into(),
                RuleFlagId::SlotUserSmsVerified => "进行归属认证后可用".into(),
                RuleFlagId::SlotUserQQVerified => "进行QQ认证后可用".into(),
                RuleFlagId::SlotAccountIsPhone => "游戏账号需为手机号".into(),
            },
            RuleFlag::Other(id) => format!("其他（{id}）"),
        }
    }
}

pub mod user_tier_availability_rank {
    pub const TIER_BASIC: i32 = 1 << 2;
    pub const TIER_SMS_VERIFIED: i32 = 1 << 1;
    pub const TIER_QQ_VERIFIED: i32 = 1 << 0;
}
