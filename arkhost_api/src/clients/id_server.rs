use crate::consts::passport::api;
use crate::models::api_passport::{LoginRequest, LoginResponse, User, UserStatus};
use crate::models::common::ResponseWrapperNested;
use reqwest::Url;
use std::{
    fmt::Debug,
    sync::{Arc, RwLock},
};

use super::common::{
    try_response_data, try_response_json, ApiResult, UnauthorizedError, UserState,
};

#[derive(Debug, Clone)]
pub struct AuthClient {
    base_url: reqwest::Url,
    user_state: Arc<RwLock<dyn UserState>>,
    pub client: reqwest::Client,
}

impl AuthClient {
    pub fn new(base_url: &str, user_state: Arc<RwLock<dyn UserState>>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-Platform",
            reqwest::header::HeaderValue::from_static(crate::consts::CLIENT_IDENTIFIER),
        );
        headers.insert(
            "User-Agent",
            reqwest::header::HeaderValue::from_static(crate::consts::CLIENT_USER_AGENT),
        );

        Self {
            base_url: Url::parse(base_url).unwrap(),
            user_state: user_state.clone(),
            client: reqwest::ClientBuilder::new()
                .default_headers(headers)
                .build()
                .unwrap(),
        }
    }

    pub async fn login(&self, email: String, password: String) -> ApiResult<User> {
        let login_request = LoginRequest { email, password };

        let resp = self
            .client
            .post(self.base_url.join(api::v1::LOGIN)?)
            .json(&login_request)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<LoginResponse>>(resp).await?;

        let login_result = try_response_data(status_code, json)?;
        self.user_state
            .write()
            .unwrap()
            .set_login_state(login_result.token.clone());
        let user_info = self.get_user_info().await?;
        if user_info.status == UserStatus::Banned {
            return Err(UnauthorizedError::BannedUser.into());
        }

        Ok(user_info)
    }

    pub fn logout(&self) {
        self.user_state.write().unwrap().erase_login_state()
    }

    pub async fn get_user_info(&self) -> ApiResult<User> {
        let resp = self
            .client
            .get(self.base_url.join(api::v1::INFO)?)
            .bearer_auth(self.get_jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<User>>(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub fn get_jwt(&self) -> Result<String, UnauthorizedError> {
        match self.user_state.read().unwrap().get_login_state() {
            Some(jwt) => Ok(jwt),
            None => Err(UnauthorizedError::MissingUserCredentials),
        }
    }

    pub async fn validate_and_update_user_state(&self) -> ApiResult<String> {
        todo!()
    }
}
