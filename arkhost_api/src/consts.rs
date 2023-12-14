pub static CLIENT_IDENTIFIER: &str = "arkhost-ui-slint";
pub static CLIENT_USER_AGENT: &str = "ArkHostApp/1.0";

pub mod passport {
    pub static API_BASE_URL: &str = "https://passport.closure.setonink.com";
    pub mod api {
        pub mod v1 {
            pub static LOGIN: &str = "/api/v1/login";
            pub static INFO: &str = "/api/v1/info";
        }
    }
}

pub mod arkhost {
    pub static API_BASE_URL: &str = "https://api.arknights.host";
    pub mod api {
        pub static GAMES: &str = "/game";

        pub fn game(account: &str) -> String {
            format!("/game/{account}")
        }
        pub fn game_log(account: &str, offset: u64) -> String {
            format!("/game/log/{account}/{offset}")
        }
        pub fn game_login(account: &str) -> String {
            format!("/game/login/{account}")
        }
        pub fn game_config(account: &str) -> String {
            format!("/game/config/{account}")
        }
    }
}

pub mod quota {
    pub static API_BASE_URL: &str = "https://quota.closure.setonink.com";
    pub mod api {}
}
