use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::config::{fs as config_fs, paths};
use crate::errors::TcampError;

pub const MANIFEST_VERSION: i64 = 1;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ManifestEntry {
    pub harness: String,
    pub created_file: bool,
    pub adopted: bool,
    pub provider_entry: Option<String>,
    pub model_ids: Vec<String>,
    pub defaults: BTreeMap<String, String>,
    pub last_written_hash: Option<String>,
    pub backups: Vec<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub path: PathBuf,
    pub targets: BTreeMap<String, ManifestEntry>,
}

impl Manifest {
    pub fn load() -> Result<Self, TcampError> {
        Self::load_from(paths::manifest_path()?)
    }

    fn load_from(path: PathBuf) -> Result<Self, TcampError> {
        let mut manifest = Self {
            path,
            targets: BTreeMap::new(),
        };

        let metadata = match fs::symlink_metadata(&manifest.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(manifest),
            Err(error) => {
                return Err(unreadable(
                    &manifest.path,
                    format!("cannot inspect the file: {error}"),
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(unreadable(&manifest.path, "refusing to follow a symlink"));
        }
        if !metadata.is_file() {
            return Err(unreadable(&manifest.path, "the path is not a regular file"));
        }

        let contents = fs::read_to_string(&manifest.path).map_err(|error| {
            unreadable(&manifest.path, format!("cannot read the file: {error}"))
        })?;
        let document = contents.parse::<DocumentMut>().map_err(|error| {
            unreadable(
                &manifest.path,
                format!("the file is not valid TOML: {error}"),
            )
        })?;

        let version = document
            .get("version")
            .and_then(Item::as_integer)
            .ok_or_else(|| unreadable(&manifest.path, "the file has no integer `version`"))?;
        if version != MANIFEST_VERSION {
            return Err(unreadable(
                &manifest.path,
                format!("unsupported manifest version {version}"),
            ));
        }

        if let Some(item) = document.get("targets") {
            let targets = item
                .as_table()
                .ok_or_else(|| unreadable(&manifest.path, "`targets` is not a table"))?;
            for (target, item) in targets.iter() {
                let table = item.as_table().ok_or_else(|| {
                    unreadable(&manifest.path, format!("target {target:?} is not a table"))
                })?;
                manifest.targets.insert(
                    target.to_string(),
                    parse_entry(&manifest.path, target, table)?,
                );
            }
        }

        Ok(manifest)
    }

    pub fn save(&self) -> Result<(), TcampError> {
        if cfg!(debug_assertions) && env::var_os("TCAMP_TEST_FAIL_MANIFEST_WRITE").is_some() {
            return Err(TcampError::io(
                format!("failed to write manifest {}", self.path.display()),
                "check that the state directory is writable",
            ));
        }

        let mut document = DocumentMut::new();
        document["version"] = Item::Value(Value::from(MANIFEST_VERSION));

        let mut targets = Table::new();
        targets.set_implicit(true);
        for (target, entry) in &self.targets {
            let mut table = Table::new();
            table["harness"] = Item::Value(Value::from(entry.harness.as_str()));
            table["created_file"] = Item::Value(Value::from(entry.created_file));
            table["adopted"] = Item::Value(Value::from(entry.adopted));
            if let Some(provider_entry) = &entry.provider_entry {
                table["provider_entry"] = Item::Value(Value::from(provider_entry.as_str()));
            }
            let mut model_ids = Array::new();
            for id in &entry.model_ids {
                model_ids.push(id.as_str());
            }
            table["model_ids"] = Item::Value(Value::Array(model_ids));
            if !entry.defaults.is_empty() {
                let mut defaults = Table::new();
                for (name, value) in &entry.defaults {
                    defaults[name.as_str()] = Item::Value(Value::from(value.as_str()));
                }
                table["defaults"] = Item::Table(defaults);
            }
            if let Some(hash) = &entry.last_written_hash {
                table["last_written_hash"] = Item::Value(Value::from(hash.as_str()));
            }
            let mut backups = Array::new();
            for backup in &entry.backups {
                backups.push(backup.as_str());
            }
            table["backups"] = Item::Value(Value::Array(backups));
            if let Some(updated_at) = &entry.updated_at {
                table["updated_at"] = Item::Value(Value::from(updated_at.as_str()));
            }
            targets[target.as_str()] = Item::Table(table);
        }
        document["targets"] = Item::Table(targets);

        let mut contents = document.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        config_fs::atomic_write(&self.path, contents.as_bytes(), Some(0o600))
    }
}

fn parse_entry(path: &Path, target: &str, table: &Table) -> Result<ManifestEntry, TcampError> {
    let harness = table
        .get("harness")
        .and_then(Item::as_str)
        .ok_or_else(|| unreadable(path, format!("target {target:?} has no string `harness`")))?;
    let created_file = table
        .get("created_file")
        .and_then(Item::as_bool)
        .unwrap_or(false);
    let adopted = table
        .get("adopted")
        .and_then(Item::as_bool)
        .unwrap_or(false);
    let provider_entry = optional_string(path, target, table, "provider_entry")?;
    let model_ids = string_array(path, target, table, "model_ids")?;
    let defaults = string_table(path, target, table, "defaults")?;
    let backups = string_array(path, target, table, "backups")?;
    let last_written_hash = optional_string(path, target, table, "last_written_hash")?;
    let updated_at = optional_string(path, target, table, "updated_at")?;

    Ok(ManifestEntry {
        harness: harness.to_string(),
        created_file,
        adopted,
        provider_entry,
        model_ids,
        defaults,
        last_written_hash,
        backups,
        updated_at,
    })
}

fn optional_string(
    path: &Path,
    target: &str,
    table: &Table,
    key: &str,
) -> Result<Option<String>, TcampError> {
    match table.get(key) {
        Some(item) => item
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| unreadable(path, format!("target {target:?} has a non-string `{key}`"))),
        None => Ok(None),
    }
}

fn string_array(
    path: &Path,
    target: &str,
    table: &Table,
    key: &str,
) -> Result<Vec<String>, TcampError> {
    let Some(item) = table.get(key) else {
        return Ok(Vec::new());
    };
    let array = item
        .as_array()
        .ok_or_else(|| unreadable(path, format!("target {target:?} has a non-array `{key}`")))?;
    array
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                unreadable(
                    path,
                    format!("target {target:?} has a non-string value in `{key}`"),
                )
            })
        })
        .collect()
}

fn string_table(
    path: &Path,
    target: &str,
    table: &Table,
    key: &str,
) -> Result<BTreeMap<String, String>, TcampError> {
    let Some(item) = table.get(key) else {
        return Ok(BTreeMap::new());
    };
    let defaults = item
        .as_table()
        .ok_or_else(|| unreadable(path, format!("target {target:?} has a non-table `{key}`")))?;
    defaults
        .iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|value| (name.to_string(), value.to_string()))
                .ok_or_else(|| {
                    unreadable(
                        path,
                        format!("target {target:?} has a non-string value in `{key}`"),
                    )
                })
        })
        .collect()
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

pub fn now() -> String {
    jiff::Timestamp::now().to_string()
}

fn unreadable(path: &Path, reason: impl std::fmt::Display) -> TcampError {
    TcampError::config_unreadable(
        format!("cannot use ownership manifest {}: {reason}", path.display()),
        format!(
            "fix or remove {} and try again; tcamp does not overwrite an unreadable manifest",
            path.display()
        ),
    )
}
