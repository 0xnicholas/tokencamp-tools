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
