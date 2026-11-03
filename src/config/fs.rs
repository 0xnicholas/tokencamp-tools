use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::errors::TcampError;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub path: PathBuf,
    pub symlink: bool,
}

#[derive(Debug, Clone)]
pub struct TargetContents {
    pub resolved: ResolvedTarget,
    pub contents: Option<Vec<u8>>,
    pub mode: Option<u32>,
}

pub fn read_target(configured: &Path) -> Result<TargetContents, TcampError> {
    let resolved = resolve_target(configured)?;
    let metadata = match fs::metadata(&resolved.path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(unreadable(
                configured,
                format!("cannot inspect the file: {error}"),
            ));
        }
    };

    let contents =
        match &metadata {
            Some(_) => Some(fs::read(&resolved.path).map_err(|error| {
                unreadable(configured, format!("cannot read the file: {error}"))
            })?),
            None => None,
        };

    #[cfg(debug_assertions)]
    maybe_mutate_after_read(&resolved.path);

    Ok(TargetContents {
        resolved,
        contents,
        mode: metadata.as_ref().map(file_mode),
    })
}

#[cfg(debug_assertions)]
fn maybe_mutate_after_read(path: &Path) {
    use std::sync::atomic::AtomicBool;

    static MUTATED: AtomicBool = AtomicBool::new(false);

    let Some(mode) = env::var_os("TCAMP_TEST_MUTATE_AFTER_READ") else {
        return;
    };
    let should_mutate = match mode.to_string_lossy().as_ref() {
        "always" => true,
        "once" => !MUTATED.swap(true, Ordering::SeqCst),
        _ => false,
    };
    if !should_mutate {
        return;
    }
    if let Ok(mut file) = OpenOptions::new().append(true).open(path) {
        let _ = file.write_all(b"\n");
    }
}

pub fn resolve_target(configured: &Path) -> Result<ResolvedTarget, TcampError> {
    let configured = absolute(configured);
    let metadata = match fs::symlink_metadata(&configured) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ResolvedTarget {
                path: configured.clone(),
                symlink: false,
            });
        }
        Err(error) => {
            return Err(unreadable(
                &configured,
                format!("cannot inspect the path: {error}"),
            ));
        }
    };

    if metadata.file_type().is_symlink() {
        let resolved = fs::canonicalize(&configured).map_err(|error| {
            unreadable(
                &configured,
                format!("refusing to follow a broken or unreadable symlink: {error}"),
            )
        })?;
        let target = fs::metadata(&resolved).map_err(|error| {
            unreadable(
                &configured,
                format!("cannot inspect the symlink target: {error}"),
            )
        })?;
        if !target.is_file() {
            return Err(unreadable(
                &configured,
                "the symlink target is not a regular file",
            ));
        }
        return Ok(ResolvedTarget {
            path: resolved,
            symlink: true,
        });
    }

    if !metadata.is_file() {
        return Err(unreadable(&configured, "the path is not a regular file"));
    }

    Ok(ResolvedTarget {
        path: configured,
        symlink: false,
    })
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

pub fn atomic_write(path: &Path, contents: &[u8], mode: Option<u32>) -> Result<(), TcampError> {
    let parent = path.parent().ok_or_else(|| {
        TcampError::io(
            format!("cannot determine the directory for {}", path.display()),
            "check the configuration path and try again",
        )
    })?;
    ensure_private_dir(parent)?;

    let temporary = temporary_path(path);
    let result = write_and_rename(&temporary, path, parent, contents, mode);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| {
        TcampError::io(
            format!("failed to write {}: {error}", path.display()),
            "check that the directory is writable and try again",
        )
    })
}

fn write_and_rename(
    temporary: &Path,
    path: &Path,
    parent: &Path,
    contents: &[u8],
    mode: Option<u32>,
) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(temporary)?;
    file.write_all(contents)?;
    file.sync_all()?;

    #[cfg(unix)]
    if let Some(mode) = mode {
        file.set_permissions(fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;

    if cfg!(debug_assertions) && env::var_os("TCAMP_TEST_FAIL_CONFIG_RENAME").is_some() {
        return Err(io::Error::other("injected config rename failure"));
    }

    fs::rename(temporary, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn write_backup(path: &Path, contents: &[u8]) -> Result<(), TcampError> {
    let parent = path.parent().ok_or_else(|| {
        TcampError::io(
            format!("cannot determine the directory for {}", path.display()),
            "check the state directory and try again",
        )
    })?;
    ensure_private_dir(parent)?;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| {
        TcampError::io(
            format!("failed to write backup {}: {error}", path.display()),
            "check that the state directory is writable and try again",
        )
    })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            TcampError::io(
                format!("failed to write backup {}: {error}", path.display()),
                "check that the state directory is writable and try again",
            )
        })?;
    Ok(())
}

pub fn ensure_private_dir(path: &Path) -> Result<(), TcampError> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        match current.symlink_metadata() {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(current.to_path_buf());
                match current.parent() {
                    Some(parent) => current = parent,
                    None => break,
                }
            }
            Err(error) => {
                return Err(unreadable(
                    path,
                    format!("cannot inspect the directory: {error}"),
                ));
            }
        }
    }

    for directory in missing.iter().rev() {
        match fs::create_dir(directory) {
            Ok(()) => {
                #[cfg(unix)]
                fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(
                    |error| {
                        unreadable(
                            directory,
                            format!("cannot set directory permissions: {error}"),
                        )
                    },
                )?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(unreadable(
                    directory,
                    format!("cannot create the directory: {error}"),
                ));
            }
        }
    }
    Ok(())
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string())
}

pub fn reject_symlink(path: &Path, label: &str) -> Result<Option<fs::Metadata>, TcampError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(TcampError::io(
                format!("cannot inspect the {label} {}: {error}", path.display()),
                "check the state directory and try again",
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(TcampError::io(
            format!("refusing to use the symlinked {label} {}", path.display()),
            "remove the symlink and try again; tcamp never follows symlinks for state files",
        ));
    }
    Ok(Some(metadata))
}

pub fn temporary_path(path: &Path) -> PathBuf {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let name = file_name(path);
    loop {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = directory.join(format!(".{name}.{}.{}.tmp", std::process::id(), counter));
        if !temporary.exists() {
            return temporary;
        }
    }
}

pub fn unreadable(path: &Path, reason: impl std::fmt::Display) -> TcampError {
    TcampError::config_unreadable(
        format!("cannot use config target {}: {reason}", path.display()),
        format!(
            "fix or remove {} and try again; tcamp does not overwrite unreadable config",
            path.display()
        ),
    )
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> u32 {
    0o600
}
