use std::ffi::{OsStr, OsString};
use tokio::process::{Child, Command};

pub fn spawn_executable(
    executable: &OsStr,
    args: &[impl AsRef<OsStr>],
    env: Option<Vec<(OsString, OsString)>>,
    inherit_env: bool,
    stdout: Option<std::process::Stdio>,
    stderr: Option<std::process::Stdio>,
) -> std::io::Result<Child> {
    let mut cmd = Command::new(executable);
    cmd.args(args);

    if !inherit_env {
        cmd.env_clear();
    }

    if let Some(env) = env {
        cmd.envs(env);
    }

    if let Some(stdout) = stdout {
        cmd.stdout(stdout);
    }

    if let Some(stderr) = stderr {
        cmd.stdout(stderr);
    }

    cmd.kill_on_drop(true).spawn()
}
