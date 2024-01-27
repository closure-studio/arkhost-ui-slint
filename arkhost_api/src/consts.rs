pub const CLIENT_IDENTIFIER: &str = "arkhost-ui-slint";
pub const CLIENT_USER_AGENT: &str = "ArkHostApp/1.0";

pub mod passport {
    pub const API_BASE_URL: &str = "https://passport.closure.setonink.com/";
    pub mod api {
        pub mod v1 {
            pub const LOGIN: &str = "api/v1/login";
            pub const INFO: &str = "api/v1/info";
        }
    }
}

pub mod arkhost {
    pub const API_BASE_URL: &str = "https://api.arknights.host/";
    pub mod api {
        pub const GAMES: &str = "game";
        pub const GAMES_SSE: &str = "sse/games";

        pub fn game(account: &str) -> String {
            format!("game/{account}")
        }
        pub fn game_log(account: &str, offset: u64) -> String {
            format!("game/log/{account}/{offset}")
        }
        pub fn game_login(account: &str) -> String {
            format!("game/login/{account}")
        }
        pub fn game_config(account: &str) -> String {
            format!("game/config/{account}")
        }

        pub mod sse {
            pub const EVENT_TYPE_GAME: &str = "game";
            pub const EVENT_TYPE_CLOSE: &str = "close";
        }
    }
}

pub mod quota {
    pub const API_BASE_URL: &str = "https://registry.closure.setonink.com";
    pub mod api {
        pub mod user {
            pub const ME: &str = "api/slots/me";
        }
        pub mod slots {
            pub const SLOTS: &str = "api/slots/slots";
            pub const GAME_ACCOUNT: &str = "api/slots/gameAccount";
        }
    }
}

pub mod asset {
    pub const API_BASE_URL: &str = "https://assets.closure.setonink.com/";
    pub const REFERER_URL: &str = "https://arknights.host";

    pub mod api {
        pub fn avatar(avatar_type: &str, id: &str) -> String {
            format!("dst/avatar/{avatar_type}/{id}")
        }

        pub fn charpack(file: &str) -> String {
            format!("dst/charpack/{file}")
        }

        pub fn gamedata(file_path: &str) -> String {
            format!("gamedata/{file_path}")
        }
    }
}