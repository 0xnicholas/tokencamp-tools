use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use jiff::Timestamp;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::errors::TcampError;

const CREDENTIALS_VERSION: i64 = 1;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct Connection {
    pub api_key: String,
    pub created_at: String,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CredentialStore {
    pub active: Option<String>,
    pub connections: BTreeMap<String, Connection>,
    pub path: PathBuf,
}

impl CredentialStore {
    pub fn load() -> Result<Self, TcampError> {
        let path = credentials_path()?;
        Self::load_from(path)
    }

    pub fn load_from(path: PathBuf) -> Result<Self, TcampError> {
        let mut store = Self {
            active: None,
            connections: BTreeMap::new(),
            path,
        };

        let metadata = match fs::symlink_metadata(&store.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(store),
            Err(error) => {
                return Err(unreadable(
                    &store.path,
                    format!("cannot inspect the file: {error}"),
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(unreadable(&store.path, "refusing to follow a symlink"));
        }
        if !metadata.file_type().is_file() {
            return Err(unreadable(&store.path, "the path is not a regular file"));
        }

        let contents = fs::read_to_string(&store.path)
            .map_err(|error| unreadable(&store.path, format!("cannot read the file: {error}")))?;
        let document = contents
            .parse::<DocumentMut>()
            .map_err(|_| unreadable(&store.path, "the file is not valid TOML"))?;

        let version = document
            .get("version")
            .and_then(Item::as_integer)
            .ok_or_else(|| unreadable(&store.path, "the file has no integer `version`"))?;
        if version != CREDENTIALS_VERSION {
            return Err(unreadable(
                &store.path,
                format!("unsupported credentials version {version}"),
            ));
        }

        store.active = match document.get("active") {
            None => None,
            Some(item) => {
                let active = item
                    .as_str()
                    .ok_or_else(|| unreadable(&store.path, "`active` is not a string"))?;
                (!active.is_empty()).then(|| active.to_string())
            }
        };

        if let Some(item) = document.get("connections") {
            let table = item
                .as_table()
                .ok_or_else(|| unreadable(&store.path, "`connections` is not a table"))?;
            for (base_url, item) in table.iter() {
                let table = item.as_table().ok_or_else(|| {
                    unreadable(
                        &store.path,
                        format!("connection {base_url:?} is not a table"),
                    )
                })?;
                let api_key = required_string(table, "api_key").ok_or_else(|| {
                    unreadable(
                        &store.path,
                        format!("connection {base_url:?} has no string `api_key`"),
                    )
                })?;
                if api_key.is_empty() {
                    return Err(unreadable(
                        &store.path,
                        format!("connection {base_url:?} has an empty `api_key`"),
                    ));
                }
                let created_at = required_string(table, "created_at").ok_or_else(|| {
                    unreadable(
                        &store.path,
                        format!("connection {base_url:?} has no string `created_at`"),
                    )
                })?;
                let verified_at = match table.get("verified_at") {
                    None => None,
                    Some(item) => Some(
                        item.as_str()
                            .ok_or_else(|| {
                                unreadable(
                                    &store.path,
                                    format!(
                                        "connection {base_url:?} has a non-string `verified_at`"
                                    ),
                                )
                            })?
                            .to_string(),
                    ),
                };
                store.connections.insert(
                    base_url.to_string(),
                    Connection {
                        api_key: api_key.to_string(),
                        created_at: created_at.to_string(),
                        verified_at,
                    },
                );
            }
        }

        Ok(store)
    }

    pub fn save(&self) -> Result<(), TcampError> {
        self.ensure_regular_target()?;

        let mut document = DocumentMut::new();
        document["version"] = value(CREDENTIALS_VERSION);
        if let Some(active) = &self.active {
            document["active"] = value(active.as_str());
        }

        let mut connections = Table::new();
        connections.set_implicit(true);
        for (base_url, connection) in &self.connections {
            let mut table = Table::new();
            table["api_key"] = value(connection.api_key.as_str());
            table["created_at"] = value(connection.created_at.as_str());
            if let Some(verified_at) = &connection.verified_at {
                table["verified_at"] = value(verified_at.as_str());
            }
            connections[base_url] = Item::Table(table);
        }
        document["connections"] = Item::Table(connections);

        let mut contents = document.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        atomic_write(&self.path, contents.as_bytes())
    }

    pub fn connection(&self, base_url: &str) -> Option<&Connection> {
        self.connections.get(base_url)
    }

    fn ensure_regular_target(&self) -> Result<(), TcampError> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(unreadable(&self.path, "refusing to replace a symlink"))
            }
            Ok(metadata) if !metadata.file_type().is_file() => {
                Err(unreadable(&self.path, "the path is not a regular file"))
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(unreadable(
                &self.path,
                format!("cannot inspect the file: {error}"),
            )),
        }
    }
}

pub fn credentials_path() -> Result<PathBuf, TcampError> {
    Ok(crate::config::paths::config_home()?
        .join("tokencamp")
        .join("credentials.toml"))
}

pub fn now() -> String {
    Timestamp::now().to_string()
}

fn required_string<'a>(table: &'a Table, key: &str) -> Option<&'a str> {
    table.get(key).and_then(Item::as_str)
}

fn value(value: impl Into<Value>) -> Item {
    Item::Value(value.into())
}

fn unreadable(path: &Path, reason: impl std::fmt::Display) -> TcampError {
    TcampError::config_unreadable(
        format!("cannot use credentials file {}: {reason}", path.display()),
        format!(
            "fix or remove {} and try again; tcamp does not overwrite unreadable credentials",
            path.display()
        ),
    )
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), TcampError> {
    let parent = path.parent().ok_or_else(|| {
        TcampError::io(
            format!("cannot determine the directory for {}", path.display()),
            "check the credentials path and try again",
        )
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        write_error(path, format!("cannot create {}: {error}", parent.display()))
    })?;
    #[cfg(unix)]
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| {
        write_error(
            path,
            format!("cannot set permissions on {}: {error}", parent.display()),
        )
    })?;

    let temporary = temporary_path(parent);
    let result = write_and_rename(&temporary, path, parent, contents);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| write_error(path, format!("cannot write the file: {error}")))
}

fn write_and_rename(
    temporary: &Path,
    path: &Path,
    parent: &Path,
    contents: &[u8],
) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(temporary)?;
    file.write_all(contents)?;
    file.sync_all()?;

    if cfg!(debug_assertions) && env::var_os("TCAMP_TEST_FAIL_CREDENTIAL_RENAME").is_some() {
        return Err(io::Error::other("injected credential rename failure"));
    }

    fs::rename(temporary, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn temporary_path(parent: &Path) -> PathBuf {
    loop {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".credentials.toml.{}.{}.tmp",
            std::process::id(),
            counter
        ));
        if !path.exists() {
            return path;
        }
    }
}

fn write_error(path: &Path, reason: String) -> TcampError {
    TcampError::io(
        format!("failed to save credentials to {}: {reason}", path.display()),
        "check that the directory is writable and try again",
    )
}
