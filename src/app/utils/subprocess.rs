use std::env;
use std::ffi::OsStr;
use subprocess::{Popen, PopenConfig, Redirection};

pub fn dup_current_exe(argv: &[impl AsRef<OsStr>]) -> subprocess::Result<Popen> {
    let current_exe = env::current_exe().unwrap_or_default();
    Popen::create(
        argv,
        PopenConfig {
            stdout: Redirection::Merge,
            stderr: Redirection::Merge,
            executable: Some(current_exe.as_os_str().to_owned()),
            ..PopenConfig::default()
        },
    )
}
