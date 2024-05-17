use reqwest::{Response, Url};
use reqwest_middleware::RequestBuilder;

use super::common;

pub struct AssetClient {
    base_url: Url,
    pub client: reqwest_middleware::ClientWithMiddleware,
}

impl AssetClient {
    pub fn new(base_url: &str, client: reqwest_middleware::ClientWithMiddleware) -> Self {
        Self {
            base_url: Url::parse(base_url).unwrap(),
            client,
        }
    }

    pub fn default_client_builder() -> reqwest::ClientBuilder {
        let client_builder = common::client_builder();
        let mut headers = common::headers();
        headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_static(crate::consts::asset::REFERER_URL),
        );
        client_builder.default_headers(headers)
    }

    pub async fn head_url_content(
        &self,
        url: Url,
        build_request: impl FnOnce(RequestBuilder) -> RequestBuilder,
    ) -> anyhow::Result<Response> {
        let request = build_request(self.client.head(url));
        Ok(request.send().await?)
    }

    pub async fn get_url_content_response(
        &self,
        url: Url,
        build_request: impl FnOnce(RequestBuilder) -> RequestBuilder,
    ) -> anyhow::Result<Response> {
        let request = build_request(self.client.get(url));
        Ok(request.send().await?)
    }

    pub async fn get_url_content(
        &self,
        url: Url,
        build_request: impl FnOnce(RequestBuilder) -> RequestBuilder,
    ) -> anyhow::Result<bytes::Bytes> {
        let bytes = self
            .get_url_content_response(url, build_request)
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        Ok(bytes)
    }

    pub async fn head_content(
        &self,
        path: &str,
        build_request: impl FnOnce(RequestBuilder) -> RequestBuilder,
    ) -> anyhow::Result<Response> {
        self.head_url_content(self.base_url.join(path)?, build_request)
            .await
    }

    pub async fn get_content_response(
        &self,
        path: &str,
        build_request: impl FnOnce(RequestBuilder) -> RequestBuilder,
    ) -> anyhow::Result<Response> {
        self.get_url_content_response(self.base_url.join(path)?, build_request)
            .await
    }

    pub async fn get_content(
        &self,
        path: &str,
        build_request: impl FnOnce(RequestBuilder) -> RequestBuilder,
    ) -> anyhow::Result<bytes::Bytes> {
        self.get_url_content(self.base_url.join(path)?, build_request)
            .await
    }
}
