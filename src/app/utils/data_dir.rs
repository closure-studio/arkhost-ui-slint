use std::{path::PathBuf, env};

static DATA_DIR: &str = ".arkhost-ui-slint";

pub fn data_dir() -> PathBuf {
    home::home_dir()
        .or(env::current_dir().ok())
        .unwrap_or(PathBuf::from("."))
        .join(DATA_DIR)
}
