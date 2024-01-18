use std::{pin::Pin, time::Duration};

use super::{
    common::{self, map_try_response_data, try_response_data, try_response_json, ApiResult},
    id_server::AuthClient,
};
use crate::models::{api_arkhost::*, common::ResponseWrapperNested};
use crate::{consts::arkhost::api, models::common::NullableData};
use anyhow::anyhow;
use es::Client as EsClient;
use eventsource_client as es;

use futures::{Stream, StreamExt, TryStreamExt};
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
        let request = UpdateGameRequest {
            config: Some(config),
            captcha_info: None,
        };
        self.update_game(account, request).await
    }

    pub async fn update_captcha_info(
        &self,
        account: &str,
        captcha_info: CaptchaResultInfo,
    ) -> ApiResult<()> {
        let request = UpdateGameRequest {
            config: None,
            captcha_info: Some(captcha_info),
        };
        self.update_game(account, request).await
    }

    pub async fn update_game(&self, account: &str, request: UpdateGameRequest) -> ApiResult<()> {
        let resp = self
            .auth_client
            .client
            .post(self.base_url.join(&api::game_config(account))?)
            .bearer_auth(self.auth_client.get_jwt()?)
            .json(&request)
            .send()
            .await?;

        let status_code = resp.status();
        let json = try_response_json::<ResponseWrapperNested<()>>(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }
}

#[derive(Debug, Clone)]
pub struct EventSourceClient {
    base_url: Url,
    auth_client: AuthClient,
}

pub type SseStream<TItem> = Pin<Box<dyn Stream<Item = TItem> + Send>>;

impl EventSourceClient {
    pub fn new(base_url: &str, auth_client: AuthClient) -> Self {
        Self {
            base_url: Url::parse(base_url).unwrap(),
            auth_client,
        }
    }

    pub fn connect_games_sse(
        &self,
    ) -> anyhow::Result<SseStream<anyhow::Result<GameSseEvent>>>
    {
        let mut url = self.base_url.join(api::GAMES_SSE)?;
        url.query_pairs_mut()
            .append_pair("token", &self.auth_client.get_jwt()?);

        let client = Self::get_default_client(url.as_str())?;
        let stream = client
            .stream()
            .map_err(anyhow::Error::from)
            .try_filter_map(|ev| async {
                match ev {
                    es::SSE::Event(ev) => match ev.event_type.as_ref() {
                        api::sse::SSE_EVENT_TYPE_GAME => {
                            let games = serde_json::de::from_str::<Vec<GameInfo>>(&ev.data)?;
                            Ok(Some(GameSseEvent::Game(games)))
                        }
                        _ => Err(anyhow!("unexpected event type {}", ev.event_type)),
                    },
                    es::SSE::Comment(_) => Ok(None), // Error on unexpected comment?
                }
            });

        Ok(stream.boxed())
    }

    pub fn get_default_client(url: &str) -> anyhow::Result<impl es::Client> {
        let mut builder = es::ClientBuilder::for_url(url)?;
        for (k, v) in common::get_common_headers() {
            if let Some(header_name) = k {
                builder = builder.header(header_name.as_str(), v.to_str()?)?;
            }
        }
        let client = builder
            .reconnect(
                es::ReconnectOptions::reconnect(true)
                    .retry_initial(true)
                    .delay(Duration::from_secs(1))
                    .backoff_factor(2)
                    .delay_max(Duration::from_secs(30))
                    .build(),
            )
            .build();

        Ok(client)
    }
}
