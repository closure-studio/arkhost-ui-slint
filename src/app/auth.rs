use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthAction {
    ArkHostRestrictedActionBackground {
        id: String,
        action: String,
    },
    ArkHostRestrictedActionCaptcha {
        id: String,
        action: String,
    },
    GeeTestAuth {
        id: String,
        gt: String,
        challenge: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthResult {
    Failed { id: String, err: String },
    ArkHostCaptchaTokenReCaptcha { id: String, token: String },
    ArkHostCaptchaTokenGeeTest { id: String, token: String },
    GeeTestAuth { id: String, token: String },
}
