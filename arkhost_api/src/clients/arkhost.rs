use std::{pin::Pin, time::Duration};

use super::{
    common::{self, map_try_response_data, try_response_data, try_response_json, ApiResult},
    id_server::AuthClient,
};
use crate::models::{api_arkhost::*, common::ResponseWrapperNested};
use crate::{consts::arkhost::api, models::common::NullableData};
use eventsource_client as es;

use futures::{Stream, StreamExt};
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
            .bearer_auth(self.auth_client.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<FetchGamesResult> = try_response_json(resp).await?;

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
            .bearer_auth(self.auth_client.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<GameDetails> = try_response_json(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn get_logs(&self, account: &str, offset: u64) -> ApiResult<GetLogResponse> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(&api::game_log(account, offset))?)
            .bearer_auth(self.auth_client.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<GetLogResponse> = try_response_json(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn login_game(&self, account: &str, captcha_token: &str) -> ApiResult<()> {
        let resp = self
            .auth_client
            .client
            .post(self.base_url.join(&api::game_login(account))?)
            .bearer_auth(self.auth_client.jwt()?)
            .header("Token", captcha_token)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<()> = try_response_json(resp).await?;

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
            .bearer_auth(self.auth_client.jwt()?)
            .json(&request)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<()> = try_response_json(resp).await?;

        Ok(try_response_data(status_code, json)?)
    }

    pub async fn get_site_config(&self) -> ApiResult<SiteConfig> {
        let resp = self
            .auth_client
            .client
            .get(self.base_url.join(api::system::CONFIG)?)
            .bearer_auth(self.auth_client.jwt()?)
            .send()
            .await?;

        let status_code = resp.status();
        let json: ResponseWrapperNested<SiteConfig> = try_response_json(resp).await?;

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

    pub fn connect_games_sse<C: es::Client>(
        &self,
        build_client: impl FnOnce(&str) -> anyhow::Result<C>,
    ) -> anyhow::Result<SseStream<anyhow::Result<GameSseEvent>>> {
        let mut url = self.base_url.join(api::sse::GAMES)?;
        url.query_pairs_mut()
            .append_pair("token", &self.auth_client.jwt()?);

        let client = build_client(url.as_str())?;
        let stream = client.stream().filter_map(|res| async {
            match res {
                Ok(ev) => match ev {
                    es::SSE::Event(ev) => Some(Self::try_parse_ev(ev)),
                    es::SSE::Comment(_) => None, // Error on unexpected comment?
                },
                Err(e) => {
                    // 先前出现读到EOF时 eventsource_client 仍可继续重试，
                    // 但仍抛出eventsource_client::Error 错误导致下游中止连接
                    // 故忽略错误并全部交由 eventsource_client::Client 重试
                    Some(Ok(GameSseEvent::RecoverableError(anyhow::Error::from(e))))
                }
            }
        });

        Ok(stream.boxed())
    }

    pub fn build_default_client(url: &str) -> anyhow::Result<impl es::Client> {
        let mut builder = es::ClientBuilder::for_url(url)?;
        for (k, v) in common::headers() {
            if let Some(header_name) = k {
                builder = builder.header(header_name.as_str(), v.to_str()?)?;
            }
        }

        let conn = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();

        let client = builder
            .reconnect(
                es::ReconnectOptions::reconnect(true)
                    .retry_initial(true)
                    .delay(Duration::from_secs(1))
                    .backoff_factor(2)
                    .delay_max(Duration::from_secs(30))
                    .build(),
            )
            .build_with_conn(conn);

        Ok(client)
    }

    fn try_parse_ev(ev: es::Event) -> anyhow::Result<GameSseEvent> {
        match ev.event_type.as_str() {
            api::sse::EVENT_TYPE_GAME => {
                let games: NullableData<Vec<GameInfo>> = serde_json::de::from_str(&ev.data)?;
                Ok(match games {
                    NullableData::Data(games) => GameSseEvent::Game(games),
                    _ => GameSseEvent::Game(Vec::default()),
                })
            }
            api::sse::EVENT_TYPE_SSR => {
                let ssr_list: Vec<SsrRecord> = serde_json::de::from_str(&ev.data)?;
                Ok(GameSseEvent::Ssr(ssr_list))
            }
            api::sse::EVENT_TYPE_CLOSE => Ok(GameSseEvent::Close),
            other => Ok(GameSseEvent::Unrecognized(other.into())),
        }
    }
}
