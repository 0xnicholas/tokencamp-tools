use std::fs;
use std::path::{Path, PathBuf};

use jiff::Timestamp;

use crate::config::{fs as config_fs, paths};
use crate::errors::TcampError;

const KEEP_BACKUPS: usize = 3;

pub fn backup_target(harness: &str, target: &Path, contents: &[u8]) -> Result<PathBuf, TcampError> {
    let directory = paths::backups_dir(harness)?;
    config_fs::ensure_private_dir(&directory)?;
    config_fs::reject_symlink(&directory, "backup directory")?;

    let file_name = config_fs::file_name(target);
    let timestamp = Timestamp::now().to_string().replace(':', "-");
    let mut path = directory.join(format!("{file_name}.{timestamp}.bak"));
    let mut counter = 0;
    while path.exists() {
        counter += 1;
        path = directory.join(format!("{file_name}.{timestamp}.{counter}.bak"));
    }

    config_fs::write_backup(&path, contents)?;
    prune(harness, target)?;
    Ok(path)
}

pub fn list_backups(harness: &str, target: &Path) -> Result<Vec<PathBuf>, TcampError> {
    let prefix = format!("{}.", config_fs::file_name(target));
    let backups = list_all(harness)?.into_iter().filter(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().starts_with(&prefix))
            .unwrap_or(false)
    });
    Ok(backups.collect())
}

pub fn list_all(harness: &str) -> Result<Vec<PathBuf>, TcampError> {
    let directory = paths::backups_dir(harness)?;
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(TcampError::io(
                format!("failed to list backups in {}: {error}", directory.display()),
                "check that the state directory is readable",
            ));
        }
    };

    let mut backups: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().ends_with(".bak"))
                .unwrap_or(false)
        })
        .collect();

    backups.sort_by(|left, right| {
        let left_key = (modified(left), left.clone());
        let right_key = (modified(right), right.clone());
        left_key.cmp(&right_key)
    });
    Ok(backups)
}

fn prune(harness: &str, target: &Path) -> Result<(), TcampError> {
    let mut backups = list_backups(harness, target)?;
    while backups.len() > KEEP_BACKUPS {
        let oldest = backups.remove(0);
        let _ = fs::remove_file(oldest);
    }
    Ok(())
}

fn modified(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::UNIX_EPOCH)
}
