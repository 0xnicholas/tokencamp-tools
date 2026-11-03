use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::errors::TcampError;
use crate::harness::Harness;

pub fn binary_path(harness: Harness) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(harness.binary()))
        .find(|candidate| is_executable(candidate))
}

pub fn require_binary(harness: Harness) -> Result<PathBuf, TcampError> {
    binary_path(harness).ok_or_else(|| {
        TcampError::harness_not_found(
            format!("{} is not installed or not on PATH", harness.binary()),
            format!(
                "install {} ({}), or re-run with `--no-launch` to write the configuration only",
                harness.binary(),
                install_url(harness)
            ),
        )
    })
}

fn install_url(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "https://docs.anthropic.com/en/docs/claude-code/setup",
        Harness::Pi => "https://github.com/badlogic/pi-mono",
        Harness::Kimi => "https://www.kimi.com/code",
    }
}

pub fn exec(
    harness: Harness,
    binary: &Path,
    args: &[OsString],
    environment: &[(String, String)],
) -> Result<(), TcampError> {
    let mut command = Command::new(binary);
    command.args(args);
    for (key, value) in environment {
        command.env(key, value);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        let error = command.exec();
        Err(TcampError::io(
            format!("failed to launch {}: {error}", harness.binary()),
            format!(
                "check that {} is executable and try again",
                binary.display()
            ),
        ))
    }

    #[cfg(not(unix))]
    {
        let status = command.status().map_err(|error| {
            TcampError::io(
                format!("failed to launch {}: {error}", harness.binary()),
                format!(
                    "check that {} is executable and try again",
                    binary.display()
                ),
            )
        })?;
        if status.success() {
            Ok(())
        } else {
            Err(TcampError::io(
                format!(
                    "{} exited with status {}",
                    harness.binary(),
                    status.code().unwrap_or(1)
                ),
                "fix the harness error and try again",
            ))
        }
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
