use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};

use arkhost_api::clients::common::UserState;

static DATA_DIR: &str = ".arkhost-ui-slint";
static FILE_NAME: &str = "user-state.token";

#[derive(Debug)]
pub enum UserStateFileStoreSetting {
    HomeDirWithCurrentDirFallback,
    #[allow(unused)]
    Path(String),
}

#[derive(Debug)]
pub struct UserStateFileStorage {
    store_setting: UserStateFileStoreSetting,
    jwt: Option<String>,
}

impl UserStateFileStorage {
    pub fn new(store_setting: UserStateFileStoreSetting) -> Self {
        UserStateFileStorage {
            jwt: None,
            store_setting,
        }
    }

    pub fn load_from_file(&mut self) {
        let path = self.get_store_path().join(FILE_NAME);
        let open_res = File::open(&path);
        match open_res {
            Ok(mut f) => {
                let mut jwt = String::new();
                if let Ok(_) = f.read_to_string(&mut jwt) {
                    self.jwt = Some(jwt);
                    println!("[UserStateFileStorage] loaded user state file from {:?}", path);
                };
            }
            Err(_) => {}
        }
    }

    pub fn save_to_file(&self) {
        if let None = self.jwt {
            return;
        }

        let dir_path = self.get_store_path();
        _ = fs::create_dir_all(&dir_path);
        let path = dir_path.join(FILE_NAME);
        match File::create(&path) {
            Ok(mut file) => match file.write_all(self.jwt.clone().unwrap().as_bytes()) {
                Ok(_) => println!("[UserStateFileStorage] user state file have been written to {:?}", path),
                Err(e) => eprintln!("[UserStateFileStorage] unable to write user state file at {:?}; Err: {e}", path),
            },
            Err(e) => eprintln!("[UserStateFileStorage] unable to create user state file at {:?}; Err: {e}", path),
        };
    }

    pub fn get_store_path(&self) -> PathBuf {
        match &self.store_setting {
            UserStateFileStoreSetting::HomeDirWithCurrentDirFallback => home::home_dir()
                .or(env::current_dir().ok())
                .unwrap_or(PathBuf::from(".")),
            UserStateFileStoreSetting::Path(path) => PathBuf::from(path),
        }
        .join(DATA_DIR)
    }
}

impl UserState for UserStateFileStorage {
    fn set_login_state(&mut self, jwt: String) {
        self.jwt = Some(jwt);
        self.save_to_file();
    }

    fn get_login_state(&self) -> Option<String> {
        self.jwt.clone()
    }

    fn erase_login_state(&mut self) {
        self.jwt = None;
        self.save_to_file();
    }
}
