use super::app_state_controller::AppStateController;
use crate::app::{app_state::model::UserConfig, utils::db};
use chrono::{DateTime, Utc};
use log::error;
use std::sync::{Arc, RwLock};

pub struct ConfigController {
    app_state_controller: Arc<AppStateController>,
    db: heed::Database<heed::types::Str, heed::types::SerdeJson<UserConfig>>,
    pub config: RwLock<UserConfig>,
}

impl ConfigController {
    pub fn new(app_state_controller: Arc<AppStateController>) -> Self {
        let db =
            db::database(Some(db::consts::db::USER_CONFIG)).expect("Failed to load user config DB");
        let config = db::env().read_txn().ok().and_then(|rtxn| {
            db.get(&rtxn, db::consts::user_config::DEFAULT_USER)
                .ok()
                .flatten()
        });
        Self {
            app_state_controller,
            db,
            config: RwLock::new(config.unwrap_or_default()),
        }
    }

    pub fn recalculate_disk_usage(&self) {
        let cache_size = match Self::cache_size() {
            Ok(size) => size,
            _ => 0u64,
        };

        let data_size = match db::env().real_disk_size() {
            Ok(size) => size,
            _ => 0u64,
        };

        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_cache_disk_usage(
                    humansize::format_size(cache_size, humansize::DECIMAL).into(),
                );
                x.set_data_disk_usage(humansize::format_size(data_size, humansize::DECIMAL).into());
            })
        });
    }

    pub fn set_clean_data(&self, val: bool) -> heed::Result<()> {
        db::request_self_delete(val)?;
        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_clean_data_requested(val);
            })
        });
        Ok(())
    }

    pub fn data_saver_mode_enabled(&self) -> bool {
        self.config.read().unwrap().data_saver_mode_enabled
    }

    pub fn set_data_saver_mode_enabled(&self, val: bool) {
        self.update_config(|c| c.data_saver_mode_enabled = val);
        self.sync_to_ui();
    }

    pub fn last_ssr_record_ts(&self) -> DateTime<Utc> {
        self.config.read().unwrap().last_ssr_record_ts
    }

    pub fn set_last_ssr_record_ts(&self, val: DateTime<Utc>) {
        self.update_config(|c| c.last_ssr_record_ts = val);
    }

    pub fn get_cached_battle_screenshots(&self, game_id: &str) -> Option<Vec<url::Url>> {
        self.config
            .read()
            .unwrap()
            .cached_battle_screenshots
            .get(game_id)
            .cloned()
    }

    pub fn set_cached_battle_screenshots(&self, game_id: String, val: Vec<url::Url>) {
        self.update_config(|c| {
            c.cached_battle_screenshots.insert(game_id, val);
        });
    }

    #[allow(unused)]
    pub fn load_from_db(&self) -> heed::Result<()> {
        let env = db::env();
        let rtxn = env.read_txn()?;
        let config: Option<UserConfig> =
            self.db.get(&rtxn, db::consts::user_config::DEFAULT_USER)?;
        if let Some(config) = config {
            *self.config.write().unwrap() = config;
        }
        Ok(())
    }

    pub fn sync_to_db(&self, config: &UserConfig) -> heed::Result<()> {
        let env = db::env();
        let mut wtxn = env.write_txn()?;
        self.db
            .put(&mut wtxn, db::consts::user_config::DEFAULT_USER, config)?;
        wtxn.commit()
    }

    pub fn sync_to_ui(&self) {
        let config = self.config.read().unwrap();
        let data_saver_mode_enabled = config.data_saver_mode_enabled;
        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_data_saver_mode_enabled(data_saver_mode_enabled);
            })
        });
    }

    fn update_config(&self, ops: impl FnOnce(&mut UserConfig)) {
        let mut config = self.config.write().unwrap();
        ops(&mut config);
        _ = self.sync_to_db(&config).map_err(|err| {
            error!("cannot save config to DB: {err}");
        });
    }

    fn cache_size() -> heed::Result<u64> {
        let mut size = 0u64;
        let env = db::env();
        let rtxn = env.read_txn()?;
        let db: heed::Database<heed::types::Bytes, heed::types::Bytes> =
            db::database(Some(db::consts::db::HTTP_CACHE))?;
        for entry in db.iter(&rtxn)? {
            let (k, v): (&[u8], &[u8]) = entry?;
            size += k.len() as u64;
            size += v.len() as u64;
        }
        Ok(size)
    }
}
