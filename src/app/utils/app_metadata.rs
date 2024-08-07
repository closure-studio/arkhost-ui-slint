use arkhost_ota;
use log::trace;
use sha2::Digest;
use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader},
    sync::{Arc, OnceLock},
};

pub const CARGO_PKG_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
pub const RELEASE_UPDATE_BRANCH: &str = arkhost_ota::consts::DEFAULT_BRANCH;

#[cfg(feature = "desktop-app")]
pub fn executable_hash<T: digest::Digest>(mut hasher: T) -> io::Result<digest::Output<T>> {
    trace!("calculating executable hash");
    let current_exe = File::open(env::current_exe()?)?;
    let mut reader = BufReader::new(current_exe);
    let mut buf;
    while {
        buf = reader.fill_buf()?;
        !buf.is_empty()
    } {
        hasher.update(buf);
        let len = buf.len();
        reader.consume(len);
    }
    Ok(hasher.finalize())
}

#[cfg(feature = "android-app")]
pub fn executable_hash<T: digest::Digest>(mut hasher: T) -> io::Result<digest::Output<T>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Executable hash is not available on Android OS",
    ))
}

pub fn executable_sha256() -> io::Result<Arc<digest::Output<sha2::Sha256>>> {
    // OnceLock::get_or_try_init 仍在 nightly 阶段
    static EXECUTABLE_HASH_SHA256_CACHED: OnceLock<Arc<digest::Output<sha2::Sha256>>> =
        OnceLock::new();
    if let Some(hash) = EXECUTABLE_HASH_SHA256_CACHED.get() {
        return Ok(hash.clone());
    }

    let hash = Arc::new(executable_hash(sha2::Sha256::new())?);
    _ = EXECUTABLE_HASH_SHA256_CACHED.set(hash.clone());
    Ok(hash)
}
