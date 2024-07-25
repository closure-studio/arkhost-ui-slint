use super::db;
use arkhost_api::clients::common::UserState;
use log::{debug, error};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct UserStateDBStore {
    jwt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Store {
    jwt: String,
}

impl UserStateDBStore {
    pub fn new() -> Self {
        UserStateDBStore { jwt: None }
    }

    pub fn load_from_db(&mut self) -> heed::Result<()> {
        let env = db::env();
        let db = Self::db()?;
        let rtxn = env.read_txn()?;
        if let Some(store) = db.get(&rtxn, db::consts::user_state::DEFAULT_USER)? {
            self.jwt = Some(store.jwt);
        }
        Ok(())
    }

    pub fn save_to_db(&self) -> heed::Result<()> {
        let jwt = match self.jwt.as_ref() {
            Some(jwt) => jwt,
            None => return Ok(()),
        };

        let env = db::env();
        let db = Self::db()?;
        let mut wtxn = env.write_txn()?;
        db.put(
            &mut wtxn,
            db::consts::user_state::DEFAULT_USER,
            &Store {
                jwt: jwt.to_owned(),
            },
        )?;
        wtxn.commit()
    }

    fn db() -> heed::Result<heed::Database<heed::types::Str, heed::types::SerdeBincode<Store>>> {
        db::database(Some(db::consts::db::USER_STATE))
    }
}

impl UserState for UserStateDBStore {
    fn set_login_state(&mut self, jwt: String) {
        self.jwt = Some(jwt);
        if let Err(e) = self.save_to_db() {
            error!("unable to write user state: {e}");
        }
    }

    fn login_state(&self) -> Option<String> {
        self.jwt.clone()
    }

    fn erase_login_state(&mut self) {
        self.jwt = None;
        if let Err(e) = self.save_to_db() {
            error!("unable to write user state: {e}");
        } else {
            debug!("user state has been written to DB");
        }
    }
}
