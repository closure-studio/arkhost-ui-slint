use reqwest::Url;

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
            "Referer",
            reqwest::header::HeaderValue::from_static(crate::consts::asset::REFERER_URL),
        );
        client_builder.default_headers(headers)
    }

    pub async fn get_content(&self, path: String) -> anyhow::Result<bytes::Bytes> {
        let response = self
            .client
            .get(self.base_url.join(&path)?)
            .send()
            .await?
            .error_for_status()?;

            let bytes = response
            .bytes()
            .await?;

        Ok(bytes)
    }
}
