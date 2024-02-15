use std::ffi::OsStr;
use subprocess::{Popen, PopenConfig, Redirection};

pub fn spawn_executable(executable: &OsStr, args: &[impl AsRef<OsStr>]) -> subprocess::Result<Popen> {
    Popen::create(
        args,
        PopenConfig {
            stdout: Redirection::Merge,
            stderr: Redirection::Merge,
            executable: Some(executable.to_owned()),
            ..PopenConfig::default()
        },
    )
}
