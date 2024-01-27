use std::collections::HashMap;
use serde_with::serde_as;

use serde::{Deserialize, Serialize};

use super::{
    api_arkhost::GamePlatform,
    api_passport::{UserPermissions, UserStatus},
};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub uuid: String,
    pub rule_flags: Vec<RuleFlag>,
    pub game_account: Option<String>,
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

#[serde_as]
#[derive(Default, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SlotRuleValidationResult {
    pub available: bool,
    #[serde_as(as = "HashMap<serde_with::json::JsonString, _>")]
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