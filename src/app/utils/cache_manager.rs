use http_cache::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use polodb_core::bson::doc;
use polodb_core::IndexModel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

use super::db;

pub struct DBCacheManager {
    collection: polodb_core::Collection<Store>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "HttpResponse")]
struct HttpResponseDef {
    #[serde(with = "serde_bytes")] // 防止body被序列化成数组
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub status: u16,
    pub url: Url,
    pub version: http_cache::HttpVersion,
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    _id: Option<polodb_core::bson::oid::ObjectId>,
    cache_key: String,
    #[serde(with = "HttpResponseDef")]
    response: HttpResponse,
    policy: CachePolicy,
}

#[allow(dead_code)]
impl DBCacheManager {
    pub fn new() -> Self {
        let col = db::instance().collection::<Store>(db::consts::collection::HTTP_CACHE);
        col.create_index(IndexModel {
            keys: doc! {
                "cache_key": 1
            },
            options: None,
        })
        .expect("Unable to create index on cache collection");
        Self { collection: col }
    }

    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheManager for DBCacheManager {
    async fn get(&self, cache_key: &str) -> Result<Option<(HttpResponse, CachePolicy)>> {
        if let Some(store) = self.collection.find_one(doc! { "cache_key": cache_key })? {
            Ok(Some((store.response, store.policy)))
        } else {
            Ok(None)
        }
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let store = Store {
            _id: None,
            cache_key: cache_key.to_owned(),
            response,
            policy,
        };

        let mut session = db::instance().start_session()?;
        session.start_transaction(Some(polodb_core::TransactionType::Write))?;
        if let Some(Store {
            _id: Some(ref oid), ..
        }) = self
            .collection
            .find_one_with_session(doc! { "cache_key": { "$eq": &cache_key } }, &mut session)?
        {
            let mut update = polodb_core::bson::to_document(&store)?;
            update.remove("_id");
            update.remove("cache_key");
            self.collection.update_one_with_session(
                doc! { "_id": oid },
                doc! {
                    "$set": update,
                },
                &mut session,
            )?;
        } else {
            self.collection
                .insert_one_with_session(&store, &mut session)?;
        };
        session.commit_transaction()?;
        Ok(store.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.collection
            .delete_many(doc! { "cache_key": cache_key })?;
        Ok(())
    }
}
