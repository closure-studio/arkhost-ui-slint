use std::{env, fs, path::PathBuf};

static DATA_DIR: &str = ".arkhost-ui-slint";

pub fn data_dir() -> PathBuf {
    if let Some(data_dir) = crate::app::env::data_dir() {
        return PathBuf::from(data_dir);
    }

    home::home_dir()
        .or(env::current_dir().ok())
        .unwrap_or(PathBuf::from("."))
        .join(DATA_DIR)
}

pub fn data_dir_create_all() -> PathBuf {
    let dir = data_dir();
    _ = fs::create_dir_all(&dir);
    dir
}
