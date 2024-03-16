use arkhost_api::clients::common::UserState;
use polodb_core::bson::doc;
use serde::{Deserialize, Serialize};

use super::db;

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

    pub fn load_from_db(&mut self) {
        let collection = db::instance().collection::<Store>(db::consts::collection::USER_STATE);
        if let Ok(Some(store)) = collection.find_one(None).map_err(|e| {
            println!("[UserStateDBStore] Error loading user state from DB: {e}");
        }) {
            self.jwt = Some(store.jwt);
        }
    }

    pub fn save_to_db(&self) -> polodb_core::Result<()> {
        let jwt = match self.jwt.as_ref() {
            Some(jwt) => jwt,
            None => return Ok(()),
        };

        let collection = db::instance().collection::<Store>(db::consts::collection::USER_STATE);
        let mut session = db::instance().start_session()?;
        session.start_transaction(None)?;
        collection.delete_one_with_session(doc! {}, &mut session)?;
        collection.insert_one_with_session(
            &Store {
                jwt: jwt.to_owned(),
            },
            &mut session,
        )?;
        session.commit_transaction()
    }
}

impl UserState for UserStateDBStore {
    fn set_login_state(&mut self, jwt: String) {
        self.jwt = Some(jwt);
        if let Err(e) = self.save_to_db() {
            println!("[UserStateDBStore] Unable to write user state: {e}");
        }
    }

    fn login_state(&self) -> Option<String> {
        self.jwt.clone()
    }

    fn erase_login_state(&mut self) {
        self.jwt = None;
        if let Err(e) = self.save_to_db() {
            println!("[UserStateDBStore] Unable to write user state: {e}");
        } else {
            println!("[UserStateDBStore] User state has been written to DB");
        }
    }
}
