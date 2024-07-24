#![allow(dead_code)]
use std::sync::OnceLock;

pub fn data_dir() -> Option<&'static str> {
    static DATA_DIR: OnceLock<Option<String>> = OnceLock::new();
    DATA_DIR
        .get_or_init(|| std::env::var(consts::DATA_DIR).ok())
        .as_ref()
        .map(|x| x.as_str())
}

pub fn attach_console() -> bool {
    static ATTACH_CONSOLE: OnceLock<bool> = OnceLock::new();
    *ATTACH_CONSOLE.get_or_init(|| std::env::var(consts::ATTACH_CONSOLE).is_ok())
}

pub fn force_update() -> bool {
    static FORCE_UPDATE: OnceLock<bool> = OnceLock::new();
    *FORCE_UPDATE.get_or_init(|| std::env::var(consts::FORCE_UPDATE).is_ok())
}

pub fn override_asset_server() -> Option<&'static str> {
    static OVERRIDE_ASSET_SERVER: OnceLock<Option<String>> = OnceLock::new();
    OVERRIDE_ASSET_SERVER
        .get_or_init(|| std::env::var(consts::OVERRIDE_ASSET_SERVER).ok())
        .as_ref()
        .map(|x| x.as_str())
}

pub fn user_token() -> Option<&'static str> {
    static USER_TOKEN: OnceLock<Option<String>> = OnceLock::new();
    USER_TOKEN
        .get_or_init(|| std::env::var(consts::USER_TOKEN).ok())
        .as_ref()
        .map(|x| x.as_str())
}

pub mod consts {
    pub const DATA_DIR: &str = "ARKHOST_APP_DATA_DIR";
    pub const ATTACH_CONSOLE: &str = "ARKHOST_APP_ATTACH_CONSOLE";
    pub const FORCE_UPDATE: &str = "ARKHOST_APP_FORCE_UPDATE";
    pub const OVERRIDE_ASSET_SERVER: &str = "ARKHOST_APP_OVERRIDE_ASSET_SERVER";
    pub const USER_TOKEN: &str = "ARKHOST_APP_USER_TOKEN";
}
