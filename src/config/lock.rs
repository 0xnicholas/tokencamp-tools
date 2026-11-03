use std::env;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use fs4::{FileExt, TryLockError};

use crate::config::{fs as config_fs, paths};
use crate::errors::TcampError;

const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

pub struct LockGuard {
    file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn acquire(harness: &str, target: &Path) -> Result<LockGuard, TcampError> {
    let directory = paths::locks_dir()?;
    config_fs::ensure_private_dir(&directory)?;
    config_fs::reject_symlink(&directory, "lock directory")?;

    let name = config_fs::file_name(target);
    let path = directory.join(format!("{harness}-{name}.lock"));
    if let Some(metadata) = config_fs::reject_symlink(&path, "lock file")? {
        if !metadata.is_file() {
            return Err(TcampError::io(
                format!("the lock file {} is not a regular file", path.display()),
                "remove the file and try again",
            ));
        }
    }

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(&path).map_err(|error| {
        TcampError::io(
            format!("failed to open the lock file {}: {error}", path.display()),
            "check that the state directory is writable and try again",
        )
    })?;

    let timeout = lock_timeout();
    let started = Instant::now();
    loop {
        match <File as FileExt>::try_lock(&file) {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => {
                if started.elapsed() >= timeout {
                    return Err(TcampError::io(
                        format!(
                            "timed out waiting for the configuration lock {}",
                            path.display()
                        ),
                        "another tcamp process is writing this configuration; try again",
                    ));
                }
                sleep(Duration::from_millis(10));
            }
            Err(TryLockError::Error(error)) => {
                return Err(TcampError::io(
                    format!(
                        "failed to lock the configuration file {}: {error}",
                        path.display()
                    ),
                    "check the state directory and try again",
                ));
            }
        }
    }

    #[cfg(debug_assertions)]
    if let Some(milliseconds) = env::var("TCAMP_TEST_LOCK_HOLD_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        sleep(Duration::from_millis(milliseconds));
    }

    Ok(LockGuard { file })
}

fn lock_timeout() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(milliseconds) = env::var("TCAMP_TEST_LOCK_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_millis(milliseconds);
    }
    DEFAULT_LOCK_TIMEOUT
}
