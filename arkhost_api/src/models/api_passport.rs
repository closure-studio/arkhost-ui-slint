use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use std::fmt::Debug;

#[derive(Default, Deserialize_repr, Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum UserStatus {
    SmsUnverified = -1,
    #[default]
    Banned = 0,
    Normal = 1,
    ManuallyVerified = 2,
    #[serde(other)]
    UnsupportedStatus = i32::MIN
}

#[derive(Default, Deserialize, Clone, Debug)]
pub struct UserPermissions(u64);

bitflags! {
    impl UserPermissions: u64 {
        const SUPER_ADMIN = 1 << 0;
        const TICKET_CREATE = 1 << 1;
        const TICKET_UPDATE = 1 << 2;
        const TICKET_OPERATE = 1 << 3;
        const CREATE_GAME = 1 << 4;
        const QUERY_GAME = 1 << 5;
        const UPDATE_GAME = 1 << 6;
        const DELETE_GAME = 1 << 7;
    }
}

#[derive(Default, Deserialize, Clone, Debug)]
pub struct User {
    #[serde(rename = "ID")]
    pub id: i32,
    #[serde(rename = "UserEmail")]
    pub user_email: String,
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "Status")]
    pub status: UserStatus,
    #[serde(rename = "IP")]
    pub ip: String,
    #[serde(rename = "Slot")]
    pub slot: i32,
    #[serde(rename = "QQ")]
    pub qq: String,
    #[serde(rename = "Phone")]
    pub phone: String,
    #[serde(rename = "Permission")]
    pub permission: UserPermissions,
}

#[derive(Default, Serialize, Clone, Debug)]
pub struct LoginRequest {
    #[serde(rename = "Email")]
    pub email: String,
    #[serde(rename = "Password")]
    pub password: String,
}

#[derive(Default, Deserialize, Clone, Debug)]
pub struct LoginResponse {
    #[serde(rename = "token")]
    pub token: String,
}
