use super::data_dir::data_dir_create_all;
use heed::types::*;
use heed::Result;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::{
    ops::Deref,
    sync::{Arc, OnceLock, RwLock, RwLockReadGuard},
};

#[derive(Debug, Serialize, Deserialize)]
struct SchemaInfo {
    pub version: semver::Version,
}

static ENV_INSTANCE: OnceLock<RwLock<Option<heed::Env>>> = OnceLock::new();
static HANDLE_SELF_DELETE: AtomicBool = AtomicBool::new(false);

pub fn env() -> EnvRef<'static> {
    let lock = env_or_none().read().unwrap();
    EnvRef { lock }
}

pub fn env_or_none() -> &'static RwLock<Option<heed::Env>> {
    ENV_INSTANCE
        .get_or_init(|| {
            let path = env_path();
            let env = try_open_env(&path)
                .unwrap_or_else(
                    || panic!("加载数据库失败！请尝试：\n1. 首次发生请关闭所有应用实例后重试\n2. 检查数据库路径是否可读写是否正确后重试\n3. 删除数据库目录\n数据库路径：{}", path.display()));
            let env = verify_schema_version_v1(env)
                .unwrap_or_else(
                    |e| panic!("校验数据库格式失败！\n{}\n 请尝试：\n1. 首次发生请关闭所有应用实例后重试\n2. 删除数据库目录\n数据库路径：{}", e, path.display()));

            RwLock::new(Some(env))
        })
}

pub fn database<K: 'static, D: 'static>(name: Option<&str>) -> Result<heed::Database<K, D>> {
    let env = env();
    let mut wtxn = env.write_txn()?;
    let db = env.create_database(&mut wtxn, name)?;
    wtxn.commit()?;
    Ok(db)
}

pub fn shutdown() {
    println!("[DB] Shutting down Env");
    let self_delete_requested = HANDLE_SELF_DELETE.load(Ordering::Relaxed)
        && match self_delete_requested() {
            Ok(val) => val,
            Err(e) => {
                println!("[DB] Error reading self delete flag: {e}");
                false
            }
        };
    let closing_event = ENV_INSTANCE
        .get()
        .and_then(|x| x.write().unwrap().take())
        .map(|x| x.prepare_for_closing());

    if let Some(closing_event) = closing_event {
        if !closing_event.wait_timeout(consts::DB_SHUTDOWN_TIMEOUT) {
            panic!("等待关闭数据库超时！");
        }
    }

    if self_delete_requested {
        let path = env_path();
        println!("[DB] Deleting DB: {path:?}");
        std::fs::remove_dir_all(path).expect("清除 App 数据失败！");
        super::notification::toast("已清除 App 数据", None, "", None);
    }
}

/// 该进程是否处理清除数据的请求
pub fn handle_self_delete(handle: bool) {
    HANDLE_SELF_DELETE.store(handle, Ordering::Relaxed);
}

pub fn request_self_delete(request: bool) -> Result<()> {
    let env = env_or_none();
    if let Some(env) = env.read().unwrap().as_ref() {
        let mut wtxn = env.write_txn()?;
        let db = env.create_database::<Str, Str>(&mut wtxn, Some(consts::db::SCHEMA_INFO))?;
        if request {
            db.put(&mut wtxn, consts::schema_info_v1::SELF_DELETE_REQUESTED, "")?;
        } else {
            db.delete(&mut wtxn, consts::schema_info_v1::SELF_DELETE_REQUESTED)?;
        }
        wtxn.commit()
    } else {
        Ok(())
    }
}

pub fn self_delete_requested() -> Result<bool> {
    let env = env_or_none();
    if let Some(env) = env.read().unwrap().as_ref() {
        let rtxn = env.read_txn()?;
        let db = env.open_database::<Str, Str>(&rtxn, Some(consts::db::SCHEMA_INFO))?;
        if let Some(db) = db {
            Ok(db
                .get(&rtxn, consts::schema_info_v1::SELF_DELETE_REQUESTED)?
                .is_some())
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

pub struct EnvRef<'a> {
    lock: RwLockReadGuard<'a, Option<heed::Env>>,
}

impl<'a> Deref for EnvRef<'a> {
    type Target = heed::Env;

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

fn env_path() -> std::path::PathBuf {
    data_dir_create_all().join(std::path::Path::new(consts::DB_DATA_PATH))
}

fn try_open_env(path: &std::path::Path) -> Option<heed::Env> {
    _ = std::fs::create_dir_all(path);

    // Safety: 见heed::EnvOpenOptions::open()
    unsafe {
        heed::EnvOpenOptions::new()
            .map_size(10 << 30)
            .max_dbs(100)
            .open(path)
    }
    .map(|db| {
        println!("[DB] Opened Env at {path:?}");
        db
    })
    .map_err(|e| println!("[DB] Error opening LMDB Env at {path:?}: {e}"))
    .ok()
}

fn verify_schema_version_v1(env: heed::Env) -> Result<heed::Env> {
    let current_schema_version = &consts::schema_info_v1::CURRENT_SCHEMA_VERSION;
    let mut wtxn = env.write_txn()?;
    let schema_info_db: heed::Database<Str, Str> =
        env.create_database(&mut wtxn, Some(consts::db::SCHEMA_INFO))?;
    let current_version = schema_info_db
        .get(&wtxn, consts::schema_info_v1::SCHEMA_VERSION_KEY)
        .ok()
        .flatten()
        .and_then(|ver| serde_json::de::from_str::<semver::Version>(ver).ok());

    if let Some(current_version) = &current_version {
        println!("[DB] Schema version found: {current_version}");
    }
    if !current_version.is_some_and(|ver| ver == consts::schema_info_v1::CURRENT_SCHEMA_VERSION) {
        println!("[DB] Dropping DBs on schema version mismatch");
        drop_dbs(&env, schema_info_db, &mut wtxn)?;

        schema_info_db.put(
            &mut wtxn,
            consts::schema_info_v1::SCHEMA_VERSION_KEY,
            &serde_json::ser::to_string(&consts::schema_info_v1::CURRENT_SCHEMA_VERSION)
                .map_err(|e| heed::Error::Encoding(e.into()))?,
        )?;
    }

    track_db_in_schema(schema_info_db, consts::db::SCHEMA_INFO, &mut wtxn)?;
    track_db_in_schema(schema_info_db, consts::db::USER_STATE, &mut wtxn)?;
    track_db_in_schema(schema_info_db, consts::db::USER_CONFIG, &mut wtxn)?;
    track_db_in_schema(schema_info_db, consts::db::HTTP_CACHE, &mut wtxn)?;
    track_db_in_schema(schema_info_db, consts::db::OTA_RELEASE, &mut wtxn)?;
    wtxn.commit()?;

    println!("[DB] Verified schema version: {}", current_schema_version);
    Ok(env)
}

fn drop_dbs(
    env: &heed::Env,
    schema_info_db: heed::Database<Str, Str>,
    wtxn: &mut heed::RwTxn,
) -> Result<()> {
    let db_names: Vec<String> = schema_info_db
        .iter(wtxn)?
        .filter_map(|x: heed::Result<(&str, &str)>| match x {
            Ok((name, _)) if name.starts_with(consts::schema_info_v1::DATABASE_INDEX_PREFIX) => {
                Some(name[consts::schema_info_v1::DATABASE_INDEX_PREFIX.len()..].to_owned())
            }
            _ => None,
        })
        .collect();
    for db_name in db_names {
        match env.open_database::<Bytes, Bytes>(wtxn, Some(&db_name)) {
            Ok(Some(db)) => {
                println!("[DB] Dropping {db_name}");
                db.clear(wtxn)?;
            }
            Err(e) => {
                println!("[DB] Unable to open DB '{db_name}' and drop: {e}");
            }
            _ => {}
        };
    }

    Ok(())
}

fn track_db_in_schema(
    schema_info_db: heed::Database<Str, Str>,
    db_name: &str,
    wtxn: &mut heed::RwTxn,
) -> Result<()> {
    let mut key = consts::schema_info_v1::DATABASE_INDEX_PREFIX.to_string();
    key.push_str(db_name);
    schema_info_db.put(wtxn, &key, "")
}

pub mod consts {
    pub const DB_DATA_PATH: &str = "heed";
    pub const DB_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

    pub mod db {
        pub const SCHEMA_INFO: &str = "__schema_info:v1";
        pub const USER_STATE: &str = "arkhost_app:user_state";
        pub const USER_CONFIG: &str = "arkhost_app:user_config";
        pub const HTTP_CACHE: &str = "arkhost_app:http_cache";
        pub const OTA_RELEASE: &str = "arkhost_app:ota_release";
    }

    pub mod schema_info_v1 {
        pub const SCHEMA_VERSION_KEY: &str = "__schema:version";
        pub const DATABASE_INDEX_PREFIX: &str = "__db_index:";
        pub const SELF_DELETE_REQUESTED: &str = "__self_delete_requested";
        pub static CURRENT_SCHEMA_VERSION: semver::Version = semver::Version::new(1, 0, 0);
    }

    pub mod user_state {
        pub const DEFAULT_USER: &str = "default_user";
    }

    pub mod user_config {
        pub const DEFAULT_USER: &str = "default_user";
    }
}
