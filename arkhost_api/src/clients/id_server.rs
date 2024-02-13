use crate::consts::passport::api;
use crate::clients::common::UserStateDataSource;
use crate::models::api_passport::{LoginRequest, LoginResponse, RefreshTokenResponse, SubmitSmsVerifyCodeRequest, User, UserStateData};
use crate::models::common::{NullableData, ResponseNestedAny, ResponseWrapperNested};
use reqwest::Url;
use std::ops::Deref;
use std::{
    fmt::Debug,
    sync::{Arc, RwLock},
};

use super::common::{
    self, map_try_response_data, try_response_data, try_response_json, ApiResult,
    UnauthorizedError, UserState,
};

#[derive(Debug, Clone)]
pub struct AuthClient {
    base_url: reqwest::Url,
    user_state: Arc<RwLock<dyn UserState>>,
    pub client: reqwest_middleware::ClientWithMiddleware,
}

impl AuthClient {
    pub fn new(
        base_url: &str,
        client: reqwest_middleware::ClientWithMiddleware,
        user_state: Arc<RwLock<dyn UserState>>,
    ) -> Self {
        Self {
            base_url: Url::parse(base_url).unwrap(),
            user_state: user_state.clone(),
            client,
        }
    }

    pub fn client_builder_with_default_settings() -> reqwest::ClientBuilder {
        let client_builder = common::client_builder_with_common_options();
        let headers = common::common_headers();
        client_builder
            .default_headers(headers)
    }

    pub async fn login(&self, email: String, password: String) -> ApiResult<()> {
        let login_request = LoginRequest { email, password };

        let resp = self
            .client
            .post(self.base_url.join(api::v1::LOGIN)?)
            .json(&login_request)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<LoginResponse> = try_response_json(resp).await?;

        let login_result = map_try_response_data(status_code, json, |data| match data {
            NullableData::Data(login_result) => Ok(login_result),
            _ => Err(data),
        })?;
        self.user_state
            .write()
            .unwrap()
            .set_login_state(login_result.token.clone());

        Ok(())
    }

    pub fn logout(&self) {
        self.user_state.write().unwrap().erase_login_state()
    }

    pub async fn get_user_info(&self) -> ApiResult<User> {
        let resp = self
            .client
            .get(self.base_url.join(api::v1::INFO)?)
            .bearer_auth(self.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<User> = try_response_json(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn submit_sms_verify_code(&self, req: &SubmitSmsVerifyCodeRequest) -> ApiResult<()> {
        let resp = self
            .client
            .post(self.base_url.join(api::v1::VERIFY_SMS)?)
            .bearer_auth(self.jwt()?)
            .json(req)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseNestedAny = try_response_json(resp).await?;
        _ = try_response_data(status_code, json)?;
        Ok(())
    }

    pub async fn get_qq_verify_code(&self) -> ApiResult<String> {
        let resp = self
            .client
            .get(self.base_url.join(api::v1::QQ_VERIFY_CODE)?)
            .bearer_auth(self.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<String> = try_response_json(resp).await?;
        Ok(try_response_data(status_code, json)?)
    }

    pub async fn refresh_token(&self) -> ApiResult<()> {
        let resp = self
            .client
            .get(self.base_url.join(api::v1::REFRESH_TOKEN)?)
            .bearer_auth(self.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<RefreshTokenResponse> = try_response_json(resp).await?;

        let resp = try_response_data(status_code, json)?;
        self.user_state.write().unwrap().set_login_state(resp.token);
        Ok(())
    }

    pub fn jwt(&self) -> Result<String, UnauthorizedError> {
        match self.user_state.read().unwrap().login_state() {
            Some(jwt) => Ok(jwt),
            None => Err(UnauthorizedError::MissingUserCredentials),
        }
    }

    pub fn user_state_data(&self) -> Option<UserStateData> {
        self.user_state.read().unwrap().deref().user_state_data()
    }
}
