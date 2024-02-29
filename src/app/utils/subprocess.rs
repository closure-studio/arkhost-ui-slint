use std::{
    env,
    ffi::{OsStr, OsString},
};
use subprocess::{Popen, PopenConfig, Redirection};

pub fn spawn_executable(
    executable: &OsStr,
    args: &[impl AsRef<OsStr>],
    mut env: Option<Vec<(OsString, OsString)>>,
    inherit_env: bool,
    stdout: Option<Redirection>,
    stderr: Option<Redirection>,
) -> subprocess::Result<Popen> {
    env = match env {
        Some(mut envs) => {
            if inherit_env {
                for (k, v) in env::vars_os() {
                    envs.push((k, v))
                }
            }

            Some(envs)
        }
        None if !inherit_env => Some(vec![]),
        _ => env,
    };

    Popen::create(
        args,
        PopenConfig {
            stdout: stdout.unwrap_or(Redirection::Merge),
            stderr: stderr.unwrap_or(Redirection::Merge),
            detached: true,
            executable: Some(executable.to_owned()),
            env,
            ..PopenConfig::default()
        },
    )
}
