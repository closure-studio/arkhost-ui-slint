use super::{
    common::{map_try_response_data, try_response_data, try_response_json, ApiResult},
    id_server::AuthClient,
};
use crate::models::{api_arkhost::*, common::ResponseWrapperNested};
use crate::{consts::arkhost::api, models::common::NullableData};

use reqwest::Url;
pub struct Client {
    base_url: Url,
    auth_client: AuthClient,
}

impl Client {
    pub fn new(base_url: &str, auth_client: AuthClient) -> Self {
        Self {
            base_url: Url::parse(base_url).unwrap(),
            auth_client,
        }
    }

    pub async fn get_games(&self) -> ApiResult<Vec<GameInfo>> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(api::GAMES)?)
            .bearer_auth(self.auth_client.get_jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<FetchGamesResult>>(resp).await?;

        Ok(map_try_response_data(status_code, json, |x| match x {
            NullableData::Data(games) => Ok(games),
            _ => Ok(vec![]),
        })?)
    }

    pub async fn get_game(&self, account: &str) -> ApiResult<GameDetails> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(&api::game(account))?)
            .bearer_auth(self.auth_client.get_jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<GameDetails>>(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn get_logs(&self, account: &str, offset: u64) -> ApiResult<GetLogResponse> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(&api::game_log(account, offset))?)
            .bearer_auth(self.auth_client.get_jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<GetLogResponse>>(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn login_game(&self, account: &str, captcha_token: &str) -> ApiResult<()> {
        let resp = self
            .auth_client
            .client
            .post(self.base_url.join(&api::game_login(account))?)
            .bearer_auth(self.auth_client.get_jwt()?)
            .header("Token", captcha_token)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<()>>(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn update_game_config(
        &self,
        account: &str,
        config: GameConfigFields,
    ) -> ApiResult<()> {
        let update_config_request = UpdateGameRequest { config };
        let resp = self
            .auth_client
            .client
            .post(self.base_url.join(&api::game_config(account))?)
            .bearer_auth(self.auth_client.get_jwt()?)
            .json(&update_config_request)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<()>>(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }
}
