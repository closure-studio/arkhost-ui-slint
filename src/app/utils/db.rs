use std::{
    ops::Deref,
    sync::{Arc, OnceLock, RwLock, RwLockReadGuard},
};

use polodb_core::Database;
use serde::{Deserialize, Serialize};

use super::data_dir::data_dir_create_all;

#[derive(Debug, Serialize, Deserialize)]
struct SchemaInfo {
    pub version: semver::Version,
}

static DB_INSTANCE: OnceLock<RwLock<Option<Database>>> = OnceLock::new();

pub fn instance() -> DatabaseRef<'static> {
    let lock = DB_INSTANCE
        .get_or_init(|| {
            let db = try_open_file(std::path::Path::new(consts::DB_DATA_PATH))
                .expect("加载数据库失败！");
            verify_schema_version(&db).expect("校验数据库版本失败！");

            RwLock::new(Some(db))
        })
        .read()
        .unwrap();
    DatabaseRef { lock }
}

pub fn shutdown() {
    println!("[DB] Shutting down DB");
    _ = DB_INSTANCE.get().and_then(|x| x.write().unwrap().take());
}

pub struct DatabaseRef<'a> {
    lock: RwLockReadGuard<'a, Option<Database>>,
}

impl<'a> Deref for DatabaseRef<'a> {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        self.lock.as_ref().expect("尝试访问已经关闭的数据库！")
    }
}

#[derive(Clone)]
pub struct CleanupGuard {
    #[allow(unused)]
    inner: Arc<InnerCleanupGuard>,
}

impl CleanupGuard {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InnerCleanupGuard {}),
        }
    }
}

struct InnerCleanupGuard {}

impl Drop for InnerCleanupGuard {
    fn drop(&mut self) {
        shutdown();
    }
}

fn try_open_file(data_path: &std::path::Path) -> Option<Database> {
    let path = data_dir_create_all().join(data_path);
    Database::open_file(&path)
        .map(|db| {
            println!("[DB] Opened DB at {path:?}");
            db
        })
        .map_err(|e| println!("[DB] Error opening Sled DB at {path:?}: {e}"))
        .ok()
}

fn drop_collections(db: &Database) {
    if let Ok(names) = db.list_collection_names() {
        for name in names {
            if let Err(e) = db.collection::<polodb_core::bson::Document>(&name).drop() {
                println!("[DB] Error dropping collection '{}' from DB: {e}", name);
            }
        }
    }
}

fn verify_schema_version(db: &Database) -> polodb_core::Result<()> {
    let current_schema_version = &consts::schema_info::CURRENT_SCHEMA_VERSION;
    let col = db.collection::<SchemaInfo>(consts::collection::SCHEMA_INFO);
    if !col
        .find_one(None)?
        .is_some_and(|x| x.version == *current_schema_version)
    {
        drop(col);

        println!("[DB] Dropping collections on schema version mismatch");
        drop_collections(db);

        let col = db.collection::<SchemaInfo>(consts::collection::SCHEMA_INFO);
        col.insert_one(&SchemaInfo {
            version: current_schema_version.clone(),
        })?;
    }

    println!("[DB] Verified schema version: {}", current_schema_version);
    Ok(())
}

pub mod consts {
    pub const DB_DATA_PATH: &str = "polodb";

    pub mod collection {
        pub const SCHEMA_INFO: &str = "__schema_info_v1";
        pub const USER_STATE: &str = "arkhost_app:user_state";
        pub const HTTP_CACHE: &str = "arkhost_app:http_cache";
        pub const OTA_RELEASE: &str = "arkhost_app:ota_release";
    }

    pub mod schema_info {
        pub static CURRENT_SCHEMA_VERSION: semver::Version = semver::Version::new(1, 0, 0);
    }
}
