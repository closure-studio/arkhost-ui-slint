use reqwest::Url;

use crate::{
    consts::quota::api,
    models::{
        api_quota::{
            Slot, SlotRuleValidationResult, UpdateSlotAccountRequest, UpdateSlotAccountResponse,
            User,
        },
        common::{ResponseWrapper, ResponseWrapperEmbed, ResponseWrapperEmbedUnion},
    },
};

use super::{
    common::{try_response_data, try_response_json, ApiResult},
    id_server::AuthClient,
};
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

    pub async fn get_user_info(&self) -> ApiResult<User> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(api::users::ME)?)
            .bearer_auth(self.auth_client.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let resp: ResponseWrapperEmbed<User> = try_response_json(resp).await?;

        Ok(try_response_data(status_code, resp)?)
    }

    pub async fn get_slots(&self) -> ApiResult<Vec<Slot>> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(api::slots::SLOTS)?)
            .bearer_auth(self.auth_client.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperEmbedUnion<Vec<Slot>> = try_response_json(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn update_slot_account(
        &self,
        uuid: &str,
        captcha_token: &str,
        request: &UpdateSlotAccountRequest,
    ) -> ApiResult<UpdateSlotAccountResponse> {
        let mut url = self.base_url.join(api::slots::GAME_ACCOUNT)?;
        url.query_pairs_mut().append_pair("uuid", uuid);

        let result: ResponseWrapperEmbed<SlotRuleValidationResult> = self
            .auth_client
            .client
            .post(url)
            .bearer_auth(self.auth_client.jwt()?)
            .header("Token", captcha_token)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        Ok(result.to_response_data())
    }
}
