use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::config::engine::Inspection;
use crate::config::manifest::ManifestEntry;
use crate::errors::TcampError;
use crate::models;

pub const HARNESS_ID: &str = "kimi";
pub const PROVIDER_ID: &str = "tokencamp";
pub const MIN_VERSION: &str = "2.1.1";
const API_KEY_ENV: &str = "TOKENCAMP_API_KEY";
const DEFAULT_ALIAS: &str = "tokencamp/glm-5.3";
const MAX_OUTPUT_SIZE: i64 = 8192;

#[derive(Debug, Clone, Copy)]
struct Alias {
    name: &'static str,
    model: &'static str,
    display_name: &'static str,
}

const ALIASES: [Alias; 5] = [
    Alias {
        name: "tokencamp/glm-5.3",
        model: models::DEFAULT_MAIN_MODEL,
        display_name: "GLM 5.3",
    },
    Alias {
        name: "tokencamp/deepseek-v4-pro",
        model: models::OPUS_MODEL,
        display_name: "DeepSeek V4 Pro",
    },
    Alias {
        name: "tokencamp/glm-5.3-flash",
        model: models::HAIKU_MODEL,
        display_name: "GLM 5.3 Flash",
    },
    Alias {
        name: "tokencamp/kimi-k2.7-code",
        model: models::KIMI_MODEL,
        display_name: "Kimi K2.7 Code",
    },
    Alias {
        name: "tokencamp/auto",
        model: "tokencamp/auto",
        display_name: "Tokencamp Auto",
    },
];

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
}

pub fn paths() -> Result<Paths, TcampError> {
    let directory = env::var_os("KIMI_CODE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".kimi-code")))
        .ok_or_else(|| {
            TcampError::config_unreadable(
                "cannot determine the Kimi Code home directory (checked KIMI_CODE_HOME and HOME)",
                "set HOME or KIMI_CODE_HOME and try again",
            )
        })?;
    Ok(Paths {
        config: directory.join("config.toml"),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelection {
    pub alias: String,
    pub model: String,
}

pub fn resolve_model(explicit: Option<&str>) -> Result<ModelSelection, TcampError> {
    let explicit = explicit.map(str::trim).map(str::to_string);
    let from_env = env::var("TOKENCAMP_MODEL")
        .ok()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty());

    match explicit.or(from_env) {
        None => {
            let alias = ALIASES
                .iter()
                .find(|alias| alias.name == DEFAULT_ALIAS)
                .expect("the default alias is in the alias table");
            Ok(selection(alias))
        }
        Some(input) => ALIASES
            .iter()
            .find(|alias| alias.name == input || alias.model == input)
            .map(selection)
            .ok_or_else(|| {
                TcampError::usage(
                    format!("{input:?} is not a model alias or canonical id exposed by the Kimi Code adapter"),
                    format!("use one of: {}", exposed_aliases().join(", ")),
                )
            }),
    }
}

fn selection(alias: &Alias) -> ModelSelection {
    ModelSelection {
        alias: alias.name.to_string(),
        model: alias.model.to_string(),
    }
}

pub fn exposed_aliases() -> Vec<&'static str> {
    ALIASES.iter().map(|alias| alias.name).collect()
}

pub fn exposed_model_ids() -> Vec<&'static str> {
    ALIASES.iter().map(|alias| alias.model).collect()
}

pub fn managed_model_ids() -> Vec<String> {
    exposed_aliases().into_iter().map(str::to_string).collect()
}

pub fn launch_args(selection: &ModelSelection, passthrough: &[OsString]) -> Vec<OsString> {
    let mut args = vec![OsString::from("-m"), OsString::from(&selection.alias)];
    args.extend(passthrough.iter().cloned());
    args
}

pub fn launch_environment(api_key: &str) -> Vec<(String, String)> {
    vec![(API_KEY_ENV.to_string(), api_key.to_string())]
}

pub fn merge_config(
    existing: Option<&[u8]>,
    path: &Path,
    base_url: &str,
) -> Result<Vec<u8>, TcampError> {
    let mut document = parse_document(existing, path)?;

    if let Some(default_model) = document.get("default_model") {
        default_model
            .as_str()
            .ok_or_else(|| structure_error(path, "`default_model` is not a string"))?;
    }
    if default_model_missing(&document) {
        set_string(document.as_table_mut(), "default_model", DEFAULT_ALIAS);
    }

    let providers = table_or_create(&mut document, "providers", path)?;
    let provider = child_table(providers, PROVIDER_ID, "providers.tokencamp", path)?;
    set_string(provider, "type", "openai");
    set_string(provider, "base_url", &crate::harness::api_base(base_url));
    set_string(provider, "api_key_env", API_KEY_ENV);
    provider.remove("api_key");

    let models_table = table_or_create(&mut document, "models", path)?;
    for alias in ALIASES {
        let entry = child_table(
            models_table,
            alias.name,
            &format!("models.\"{}\"", alias.name),
            path,
        )?;
        let catalog = models::catalog_model(alias.model)
            .expect("every exposed alias maps to a catalog model");
        set_string(entry, "provider", PROVIDER_ID);
        set_string(entry, "model", alias.model);
        set_integer(entry, "max_context_size", catalog.context_window as i64);
        set_integer(entry, "max_output_size", MAX_OUTPUT_SIZE);
        set_string(entry, "display_name", alias.display_name);
    }

    validate_structure(&document, path)?;
    validate_references(&document, path)?;

    let mut text = document.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text.into_bytes())
}

pub fn inspect_status(
    existing: Option<&[u8]>,
    path: &Path,
    entry: Option<&ManifestEntry>,
) -> Result<crate::harness::StatusSnapshot, TcampError> {
    let Some(contents) = existing else {
        return Ok(crate::harness::StatusSnapshot::default());
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(crate::harness::StatusSnapshot::default());
    }

    let document = parse_document(Some(contents), path)?;
    validate_structure(&document, path)?;

    let provider_present = document
        .get("providers")
        .and_then(Item::as_table)
        .is_some_and(|providers| providers.contains_key(PROVIDER_ID));
    let aliases_present = managed_aliases_present(&document);
    if !provider_present && aliases_present.is_empty() {
        return Ok(crate::harness::StatusSnapshot::default());
    }

    let owned = entry
        .and_then(|entry| entry.provider_entry.as_deref())
        .is_some_and(|provider| provider == PROVIDER_ID);
    Ok(crate::harness::StatusSnapshot {
        configured: owned,
        conflict: !owned,
    })
}

pub fn plan_removal(
    path: &Path,
    existing: &crate::config::TargetContents,
    entry: Option<&ManifestEntry>,
    force: bool,
) -> Result<crate::uninstall::Removal, TcampError> {
    use crate::uninstall::Removal;

    let Some(contents) = existing.contents.as_deref() else {
        return Ok(Removal::nothing(
            "providers.tokencamp and model aliases",
            "config.toml no longer exists",
        ));
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(Removal::nothing(
            "providers.tokencamp and model aliases",
            "config.toml is empty",
        ));
    }

    let mut document = parse_document(Some(contents), path)?;
    validate_structure(&document, path)?;

    let provider_present = document
        .get("providers")
        .and_then(Item::as_table)
        .is_some_and(|providers| providers.contains_key(PROVIDER_ID));
    let aliases_present = managed_aliases_present(&document);
    if !provider_present && aliases_present.is_empty() {
        return Ok(Removal::nothing(
            "providers.tokencamp and model aliases",
            "there are no tokencamp entries",
        ));
    }

    let owned = entry
        .and_then(|entry| entry.provider_entry.as_deref())
        .is_some_and(|provider| provider == PROVIDER_ID);
    let hash = crate::config::manifest::hash_bytes(contents);
    let modified =
        entry.is_some_and(|entry| entry.last_written_hash.as_deref() != Some(hash.as_str()));
    let adopted = entry.is_some_and(|entry| entry.adopted);

    if !owned && !force {
        return Ok(Removal::keep(
            "providers.tokencamp and model aliases",
            "config_conflict",
            "the Kimi Code `tokencamp` entries are not recorded as managed by tcamp; re-run with `--force` to remove them",
        ));
    }
    if owned && adopted && !force {
        return Ok(Removal::keep(
            "providers.tokencamp and model aliases",
            "config_conflict",
            "the Kimi Code entries were adopted from existing entries; re-run with `--force` to remove them",
        ));
    }
    if owned && modified && !force {
        return Ok(Removal::keep(
            "providers.tokencamp and model aliases",
            "config_drift",
            "config.toml was modified outside tcamp; re-run with `--force` to remove the tokencamp entries",
        ));
    }

    if let Some(providers) = document.get_mut("providers").and_then(Item::as_table_mut) {
        providers.remove(PROVIDER_ID);
    }

    let names: Vec<String> = match entry {
        Some(entry) if owned && !entry.model_ids.is_empty() => entry.model_ids.clone(),
        _ => document
            .get("models")
            .and_then(Item::as_table)
            .map(|models| {
                models
                    .iter()
                    .map(|(name, _)| name.to_string())
                    .filter(|name| {
                        name.starts_with("tokencamp/")
                            || ALIASES.iter().any(|alias| alias.name == name)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    if let Some(models) = document.get_mut("models").and_then(Item::as_table_mut) {
        for name in &names {
            models.remove(name);
        }
    }

    match entry {
        Some(entry) => {
            if let Some(value) = entry.defaults.get("default_model") {
                let current = document.get("default_model").and_then(Item::as_str);
                if force || current == Some(value.as_str()) {
                    document.as_table_mut().remove("default_model");
                }
            }
        }
        None if force => {
            let managed = document
                .get("default_model")
                .and_then(Item::as_str)
                .is_some_and(|value| {
                    value.starts_with("tokencamp/")
                        || ALIASES.iter().any(|alias| alias.name == value)
                });
            if managed {
                document.as_table_mut().remove("default_model");
            }
        }
        None => {}
    }

    for table in ["providers", "models"] {
        let empty = document
            .get(table)
            .and_then(Item::as_table)
            .is_some_and(Table::is_empty);
        if empty {
            document.as_table_mut().remove(table);
        }
    }

    if document.as_table().is_empty()
        && entry.is_some_and(|entry| entry.created_file)
        && !existing.resolved.symlink
    {
        return Ok(Removal::delete("config.toml"));
    }

    let mut text = document.to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(Removal::rewrite(
        "providers.tokencamp and model aliases",
        text.into_bytes(),
    ))
}

pub fn inspect_config(
    existing: Option<&[u8]>,
    path: &Path,
    entry: Option<&ManifestEntry>,
    force: bool,
) -> Result<Inspection, TcampError> {
    let owned = entry
        .and_then(|entry| entry.provider_entry.as_deref())
        .is_some_and(|provider| provider == PROVIDER_ID);

    let Some(contents) = existing else {
        return Ok(inspection(
            false,
            false,
            managed_model_ids(),
            written_defaults(None),
        ));
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(inspection(
            false,
            false,
            managed_model_ids(),
            written_defaults(None),
        ));
    }

    let document = parse_document(Some(contents), path)?;
    validate_structure(&document, path)?;

    let provider_present = document
        .get("providers")
        .and_then(Item::as_table)
        .is_some_and(|providers| providers.contains_key(PROVIDER_ID));
    let aliases_present = managed_aliases_present(&document);
    let unowned_entry = !owned && (provider_present || !aliases_present.is_empty());

    if unowned_entry && !force {
        return Err(unowned_conflict(path));
    }

    let drift = owned
        && entry
            .and_then(|entry| entry.last_written_hash.as_deref())
            .is_some_and(|previous| previous != crate::config::manifest::hash_bytes(contents));
    let model_ids = if owned {
        entry
            .map(|entry| entry.model_ids.clone())
            .unwrap_or_default()
    } else {
        adopted_model_ids(&aliases_present)
    };
    let defaults = written_defaults(Some(&document));

    Ok(inspection(drift, unowned_entry, model_ids, defaults))
}

pub fn inspect_literal_key(
    existing: Option<&[u8]>,
    path: &Path,
    entry: Option<&ManifestEntry>,
) -> Result<LiteralKey, TcampError> {
    let Some(contents) = existing else {
        return Ok(LiteralKey::Absent);
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(LiteralKey::Absent);
    }

    let document = parse_document(Some(contents), path)?;
    validate_structure(&document, path)?;
    let provider = document
        .get("providers")
        .and_then(Item::as_table)
        .and_then(|providers| providers.get(PROVIDER_ID))
        .and_then(Item::as_table);
    let Some(provider) = provider else {
        return Ok(LiteralKey::Absent);
    };
    let Some(api_key) = provider.get("api_key") else {
        return Ok(LiteralKey::Absent);
    };
    let Some(value) = api_key.as_str() else {
        return Ok(LiteralKey::Invalid);
    };
    if value.trim().is_empty() {
        return Ok(LiteralKey::Absent);
    }

    let owned = entry.and_then(|entry| entry.provider_entry.as_deref()) == Some(PROVIDER_ID);
    Ok(if owned {
        LiteralKey::Owned
    } else {
        LiteralKey::Unowned
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKey {
    Absent,
    Owned,
    Unowned,
    Invalid,
}

fn inspection(
    drift: bool,
    adopted: bool,
    model_ids: Vec<String>,
    defaults: BTreeMap<String, String>,
) -> Inspection {
    Inspection {
        drift,
        adopted,
        model_ids,
        provider_entry: Some(PROVIDER_ID.to_string()),
        defaults,
    }
}

fn written_defaults(document: Option<&DocumentMut>) -> BTreeMap<String, String> {
    let mut defaults = BTreeMap::new();
    let missing = document.is_none_or(default_model_missing);
    if missing {
        defaults.insert("default_model".to_string(), DEFAULT_ALIAS.to_string());
    }
    defaults
}

fn default_model_missing(document: &DocumentMut) -> bool {
    document
        .get("default_model")
        .is_none_or(|item| item.as_str().is_none_or(|value| value.trim().is_empty()))
}

fn managed_aliases_present(document: &DocumentMut) -> Vec<&'static str> {
    let models = match document.get("models").and_then(Item::as_table) {
        Some(models) => models,
        None => return Vec::new(),
    };
    ALIASES
        .iter()
        .map(|alias| alias.name)
        .filter(|name| models.contains_key(name))
        .collect()
}

fn adopted_model_ids(existing: &[&str]) -> Vec<String> {
    ALIASES
        .iter()
        .map(|alias| alias.name)
        .filter(|name| !existing.contains(name))
        .map(str::to_string)
        .collect()
}

fn parse_document(existing: Option<&[u8]>, path: &Path) -> Result<DocumentMut, TcampError> {
    let Some(contents) = existing else {
        return Ok(DocumentMut::new());
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(DocumentMut::new());
    }
    let text = std::str::from_utf8(contents).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    text.parse::<DocumentMut>().map_err(|error| {
        let detail = crate::config::diff::redact(error.message());
        match error.span() {
            Some(span) => {
                let (line, column) = line_column(text, span.start);
                crate::config::fs::unreadable(
                    path,
                    format!("the file is not valid TOML at line {line}, column {column}: {detail}"),
                )
            }
            None => {
                crate::config::fs::unreadable(path, format!("the file is not valid TOML: {detail}"))
            }
        }
    })
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map(|line| line.chars().count() + 1)
        .unwrap_or(1);
    (line, column)
}

fn validate_structure(document: &DocumentMut, path: &Path) -> Result<(), TcampError> {
    if let Some(providers) = document.get("providers") {
        let providers = providers
            .as_table()
            .ok_or_else(|| structure_error(path, "`providers` is not a table"))?;
        if let Some(provider) = providers.get(PROVIDER_ID) {
            provider
                .as_table()
                .ok_or_else(|| structure_error(path, "`providers.tokencamp` is not a table"))?;
        }
    }

    if let Some(models) = document.get("models") {
        let models = models
            .as_table()
            .ok_or_else(|| structure_error(path, "`models` is not a table"))?;
        for alias in ALIASES {
            if let Some(entry) = models.get(alias.name) {
                entry.as_table().ok_or_else(|| {
                    structure_error(path, format!("`models.\"{}\"` is not a table", alias.name))
                })?;
            }
        }
    }

    if let Some(default_model) = document.get("default_model") {
        default_model
            .as_str()
            .ok_or_else(|| structure_error(path, "`default_model` is not a string"))?;
    }

    Ok(())
}

fn validate_references(document: &DocumentMut, path: &Path) -> Result<(), TcampError> {
    let providers = document
        .get("providers")
        .and_then(Item::as_table)
        .ok_or_else(|| structure_error(path, "`providers` is not a table"))?;
    if !providers.contains_key(PROVIDER_ID) {
        return Err(structure_error(
            path,
            format!("`providers.{PROVIDER_ID}` is missing"),
        ));
    }

    let models = document
        .get("models")
        .and_then(Item::as_table)
        .ok_or_else(|| structure_error(path, "`models` is not a table"))?;
    for alias in ALIASES {
        let entry = models
            .get(alias.name)
            .and_then(Item::as_table)
            .ok_or_else(|| {
                structure_error(
                    path,
                    format!("`models.\"{}\"` is missing or not a table", alias.name),
                )
            })?;
        if entry.get("provider").and_then(Item::as_str) != Some(PROVIDER_ID) {
            return Err(structure_error(
                path,
                format!(
                    "`models.\"{}\".provider` does not reference `providers.{PROVIDER_ID}`",
                    alias.name
                ),
            ));
        }
        if entry.get("model").and_then(Item::as_str) != Some(alias.model)
            || models::catalog_model(alias.model).is_none()
        {
            return Err(structure_error(
                path,
                format!(
                    "`models.\"{}\".model` does not reference a known Catalog model",
                    alias.name
                ),
            ));
        }
    }

    Ok(())
}

fn table_or_create<'a>(
    document: &'a mut DocumentMut,
    key: &str,
    path: &Path,
) -> Result<&'a mut Table, TcampError> {
    if document.get(key).is_none() {
        let mut table = Table::new();
        table.set_implicit(true);
        document[key] = Item::Table(table);
    }
    document
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| structure_error(path, format!("`{key}` is not a table")))
}

fn child_table<'a>(
    parent: &'a mut Table,
    key: &str,
    label: &str,
    path: &Path,
) -> Result<&'a mut Table, TcampError> {
    if parent.get(key).is_none() {
        parent.insert(key, Item::Table(Table::new()));
    }
    parent
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| structure_error(path, format!("`{label}` is not a table")))
}

fn set_string(table: &mut Table, key: &str, value: &str) {
    set_value(table, key, Value::from(value));
}

fn set_integer(table: &mut Table, key: &str, value: i64) {
    set_value(table, key, Value::from(value));
}

fn set_value(table: &mut Table, key: &str, mut value: Value) {
    match table.get_mut(key) {
        Some(Item::Value(existing)) => {
            *value.decor_mut() = existing.decor().clone();
            *existing = value;
        }
        Some(item) => *item = Item::Value(value),
        None => {
            table.insert(key, Item::Value(value));
        }
    }
}

pub fn unowned_conflict(path: &Path) -> TcampError {
    TcampError::config_conflict(
        format!(
            "the Kimi Code provider entry or model aliases `tokencamp` in {} are not managed by tcamp",
            path.display()
        ),
        "re-run with `--force` to back up the file and adopt the entries; tcamp never overwrites unowned entries without confirmation",
    )
}

fn structure_error(path: &Path, reason: impl std::fmt::Display) -> TcampError {
    crate::config::fs::unreadable(path, reason)
}
