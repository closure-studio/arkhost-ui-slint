use super::db;
use http_cache::{CacheManager, HttpResponse, Result};
use http_cache_semantics::CachePolicy;
use log::trace;
use serde::{Deserialize, Serialize};

pub struct DBCacheManager {
    db: heed::Database<heed::types::Str, heed::types::SerdeBincode<Store>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    cache_key: String,
    response: HttpResponse,
    policy: CachePolicy,
}

#[allow(dead_code)]
impl DBCacheManager {
    pub fn new() -> Self {
        let db = db::database(Some(db::consts::db::HTTP_CACHE))
            .expect("Unable to create index on cache collection");
        Self { db }
    }

    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        let env = db::env();
        let mut wtxn = env.write_txn().map_err(into_box_error)?;
        self.db.clear(&mut wtxn).map_err(into_box_error)?;
        wtxn.commit().map_err(into_box_error)
    }
}

#[async_trait::async_trait]
impl CacheManager for DBCacheManager {
    async fn get(&self, cache_key: &str) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let env = db::env();
        let rtxn = env.read_txn().map_err(into_box_error)?;
        self.db
            .get(&rtxn, cache_key)
            .map(|x| {
                trace!(
                    "retrieving '{cache_key}' found: {}, body size: {}B",
                    x.is_some(),
                    x.as_ref().map_or(0usize, |x| x.response.body.len())
                );
                x.map(|x| (x.response, x.policy))
            })
            .map_err(into_box_error)
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        trace!("storing '{cache_key}', body size: {}B", response.body.len());
        let store = Store {
            cache_key: cache_key.to_owned(),
            response,
            policy,
        };

        let env = db::env();
        let mut wtxn = env.write_txn().map_err(into_box_error)?;
        self.db
            .put(&mut wtxn, &cache_key, &store)
            .map_err(into_box_error)?;
        wtxn.commit().map_err(into_box_error)?;
        Ok(store.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        trace!("removing '{cache_key}'");
        let env = db::env();
        let mut wtxn = env.write_txn().map_err(into_box_error)?;
        self.db
            .delete(&mut wtxn, cache_key)
            .map_err(into_box_error)?;
        wtxn.commit().map_err(into_box_error)
    }
}

fn into_box_error(e: heed::Error) -> http_cache::BoxError {
    anyhow::anyhow!(e.to_string()).into()
}
