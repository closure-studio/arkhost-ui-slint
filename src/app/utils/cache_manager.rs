use std::path::PathBuf;

use cacache::RemoveOpts;
use http_cache::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: PathBuf,
}

impl Default for CACacheManager {
    fn default() -> Self {
        Self {
            path: "./http-cacache".into(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

#[allow(dead_code)]
impl CACacheManager {
    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        cacache::clear(&self.path).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheManager for CACacheManager {
    async fn get(&self, cache_key: &str) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match cacache::read(&self.path, cache_key).await {
            Ok(d) => bincode::deserialize(&d)?,
            Err(_e) => {
                return Ok(None);
            }
        };
        Ok(Some((store.response, store.policy)))
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store {
            response: response.clone(),
            policy,
        };
        let bytes = bincode::serialize(&data)?;
        _ = self.delete(&cache_key).await;
        cacache::write(&self.path, cache_key, bytes).await?;
        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        while let Ok(Some(metadata)) = cacache::metadata(&self.path, cache_key).await {
            if cacache::exists(&self.path, &metadata.integrity).await {
                if let Err(e) = cacache::remove_hash(&self.path, &metadata.integrity).await {
                    println!("[CACacheManager] failed to remove cache content: {e}");
                    break;
                }
            } else if let Err(e) = cacache::remove(&self.path, cache_key).await {
                println!("[CACacheManager] failed to remove cache entry '{cache_key}': {e}");
                break;
            }
        }

        Ok(RemoveOpts::new()
            .remove_fully(true)
            .remove(&self.path, cache_key)
            .await?)
    }
}
