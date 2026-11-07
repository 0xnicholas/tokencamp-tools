use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{
    CstArray, CstContainerNode, CstInputValue, CstLeafNode, CstNode, CstObject, CstObjectProp,
    CstRootNode,
};
use serde::Deserialize;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde_json::Value;

use crate::config::engine::Inspection;
use crate::config::manifest::ManifestEntry;
use crate::errors::TcampError;
use crate::harness::api_base;
use crate::models;
use crate::warnings::Warning;

pub const HARNESS_ID: &str = "pi";
pub const PROVIDER_ID: &str = "tokencamp";
pub const MIN_VERSION: &str = "1.4.0";
const MODELS_SCHEMA: &str = "https://pi.dev/schemas/models.schema.json";
const API_KEY_REFERENCE: &str = "$TOKENCAMP_API_KEY";

const MODEL_NAMES: [(&str, &str); 5] = [
    (models::DEFAULT_MAIN_MODEL, "GLM 5.3"),
    (models::OPUS_MODEL, "DeepSeek V4 Pro"),
    (models::HAIKU_MODEL, "GLM 5.3 Flash"),
    (models::KIMI_MODEL, "Kimi K2.7 Code"),
    ("tokencamp/auto", "Tokencamp Auto"),
];

const MODEL_ALIASES: [(&str, &str); 5] = [
    ("glm-5.3", models::DEFAULT_MAIN_MODEL),
    ("deepseek-v4-pro", models::OPUS_MODEL),
    ("glm-5.3-flash", models::HAIKU_MODEL),
    ("kimi-k2.7-code", models::KIMI_MODEL),
    ("auto", "tokencamp/auto"),
];

#[derive(Debug, Clone)]
pub struct Paths {
    pub models: PathBuf,
    pub settings: PathBuf,
    pub auth: PathBuf,
}

pub fn paths() -> Result<Paths, TcampError> {
    let directory = agent_dir()?;
    Ok(Paths {
        models: directory.join("models.json"),
        settings: directory.join("settings.json"),
        auth: directory.join("auth.json"),
    })
}

fn agent_dir() -> Result<PathBuf, TcampError> {
    env::var_os("PI_CODING_AGENT_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".pi").join("agent")))
        .ok_or_else(|| {
            TcampError::config_unreadable(
                "cannot determine the Pi agent directory (checked PI_CODING_AGENT_DIR and HOME)",
                "set HOME or PI_CODING_AGENT_DIR and try again",
            )
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelection {
    pub model: String,
}

pub fn resolve_model(explicit: Option<&str>) -> Result<ModelSelection, TcampError> {
    let explicit = explicit.map(str::trim).map(str::to_string);
    let from_env = env::var("TOKENCAMP_MODEL")
        .ok()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty());

    match explicit.or(from_env) {
        None => Ok(ModelSelection {
            model: models::DEFAULT_MAIN_MODEL.to_string(),
        }),
        Some(input) => {
            let canonical = canonical_model(&input).ok_or_else(|| {
                TcampError::usage(
                    format!("{input:?} is not a model exposed by the Pi adapter"),
                    format!("use one of: {}", exposed_ids().join(", ")),
                )
            })?;
            Ok(ModelSelection {
                model: canonical.to_string(),
            })
        }
    }
}

fn canonical_model(input: &str) -> Option<&'static str> {
    if let Some(model) = models::catalog_model(input) {
        return Some(model.id);
    }
    MODEL_ALIASES
        .iter()
        .find(|(alias, _)| *alias == input)
        .map(|(_, id)| *id)
}

pub fn exposed_ids() -> Vec<&'static str> {
    models::EXPOSED_MODELS
        .iter()
        .map(|model| model.id)
        .collect()
}

pub fn managed_model_ids() -> Vec<String> {
    exposed_ids().into_iter().map(str::to_string).collect()
}

pub fn launch_args(selection: &ModelSelection, passthrough: &[OsString]) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--provider"),
        OsString::from(PROVIDER_ID),
        OsString::from("--model"),
        OsString::from(&selection.model),
    ];
    args.extend(passthrough.iter().cloned());
    args
}

pub fn launch_environment(api_key: &str) -> Vec<(String, String)> {
    vec![("TOKENCAMP_API_KEY".to_string(), api_key.to_string())]
}

pub fn merge_models(
    existing: Option<&[u8]>,
    path: &Path,
    base_url: &str,
) -> Result<Vec<u8>, TcampError> {
    let Some(text) = existing else {
        return Ok(new_models_document(base_url).into_bytes());
    };
    let text = std::str::from_utf8(text).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    if text.trim().is_empty() {
        return Ok(new_models_document(base_url).into_bytes());
    }

    let root = CstRootNode::parse(text, &ParseOptions::default()).map_err(|error| {
        crate::config::fs::unreadable(path, format!("cannot parse the file: {error}"))
    })?;
    let document = root
        .object_value()
        .ok_or_else(|| structure_error(path, "the root value is not an object"))?;
    let providers = document
        .object_value_or_create("providers")
        .ok_or_else(|| structure_error(path, "`providers` is not an object"))?;

    let provider = match providers.get(PROVIDER_ID) {
        Some(_) => providers
            .object_value_or_create(PROVIDER_ID)
            .ok_or_else(|| structure_error(path, "`providers.tokencamp` is not an object"))?,
        None => {
            providers.append(PROVIDER_ID, CstInputValue::Object(Vec::new()));
            providers
                .object_value(PROVIDER_ID)
                .expect("the provider entry was just created")
        }
    };

    set_property(&provider, "name", CstInputValue::from("tokencamp"));
    set_property(
        &provider,
        "baseUrl",
        CstInputValue::from(api_base(base_url)),
    );
    set_property(&provider, "api", CstInputValue::from("openai-completions"));
    set_property(&provider, "apiKey", CstInputValue::from(API_KEY_REFERENCE));
    upsert_compat(&provider);

    let models = match provider.get("models") {
        Some(prop) => prop
            .value()
            .and_then(|value| value.as_array())
            .ok_or_else(|| structure_error(path, "`providers.tokencamp.models` is not an array"))?,
        None => {
            provider.append("models", CstInputValue::Array(Vec::new()));
            provider
                .array_value("models")
                .expect("the models array was just created")
        }
    };

    for id in exposed_ids() {
        match find_model(&models, id) {
            Some(model) => upsert_model(&model, id),
            None => {
                models.append(model_value(id));
            }
        }
    }

    Ok(root.to_string().into_bytes())
}

fn new_models_document(base_url: &str) -> String {
    let root = CstRootNode::parse("{}", &ParseOptions::default())
        .expect("an empty JSON object is always valid");
    let document = root
        .object_value_or_create()
        .expect("the root object was just created");
    document.append("$schema", CstInputValue::from(MODELS_SCHEMA));
    document.append(
        "providers",
        CstInputValue::Object(vec![(PROVIDER_ID.to_string(), provider_value(base_url))]),
    );
    let mut text = root.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn provider_value(base_url: &str) -> CstInputValue {
    CstInputValue::Object(vec![
        ("name".to_string(), CstInputValue::from("tokencamp")),
        (
            "baseUrl".to_string(),
            CstInputValue::from(api_base(base_url)),
        ),
        ("api".to_string(), CstInputValue::from("openai-completions")),
        ("apiKey".to_string(), CstInputValue::from(API_KEY_REFERENCE)),
        ("compat".to_string(), compat_value()),
        (
            "models".to_string(),
            CstInputValue::Array(exposed_ids().into_iter().map(model_value).collect()),
        ),
    ])
}

fn compat_fields() -> Vec<(String, CstInputValue)> {
    vec![
        (
            "maxTokensField".to_string(),
            CstInputValue::from("max_tokens"),
        ),
        (
            "supportsDeveloperRole".to_string(),
            CstInputValue::Bool(false),
        ),
        ("supportsStore".to_string(), CstInputValue::Bool(false)),
        (
            "supportsReasoningEffort".to_string(),
            CstInputValue::Bool(false),
        ),
    ]
}

fn compat_value() -> CstInputValue {
    CstInputValue::Object(compat_fields())
}

fn model_fields(id: &str) -> Vec<(String, CstInputValue)> {
    let catalog = models::catalog_model(id).expect("every exposed model is in the catalog");
    let mut input = vec![CstInputValue::from("text")];
    if id == models::HAIKU_MODEL {
        input.push(CstInputValue::from("image"));
    }

    vec![
        ("id".to_string(), CstInputValue::from(id)),
        ("name".to_string(), CstInputValue::from(model_name(id))),
        ("input".to_string(), CstInputValue::Array(input)),
        (
            "contextWindow".to_string(),
            CstInputValue::Number(catalog.context_window.to_string()),
        ),
        (
            "maxTokens".to_string(),
            CstInputValue::Number(catalog.max_output.to_string()),
        ),
        ("compat".to_string(), compat_value()),
    ]
}

fn model_value(id: &str) -> CstInputValue {
    CstInputValue::Object(model_fields(id))
}

fn model_name(id: &str) -> String {
    MODEL_NAMES
        .iter()
        .find(|(model_id, _)| *model_id == id)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| id.to_string())
}

fn upsert_model(model: &CstObject, id: &str) {
    for (name, value) in model_fields(id) {
        if name == "compat" {
            upsert_compat(model);
        } else {
            set_property(model, &name, value);
        }
    }
}

fn upsert_compat(object: &CstObject) {
    match object.object_value_or_create("compat") {
        Some(compat) => {
            for (name, value) in compat_fields() {
                set_property(&compat, &name, value);
            }
        }
        None => {
            if let Some(prop) = object.get("compat") {
                prop.set_value(compat_value());
            }
        }
    }
}

fn set_property(object: &CstObject, name: &str, value: CstInputValue) {
    match object.get(name) {
        Some(property) => property.set_value(value),
        None => {
            object.append(name, value);
        }
    }
}

fn find_model(models: &CstArray, id: &str) -> Option<CstObject> {
    models
        .elements()
        .into_iter()
        .filter_map(|element| element.as_object())
        .find(|model| string_property(model, "id").as_deref() == Some(id))
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

    let text = std::str::from_utf8(contents).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    let root = CstRootNode::parse(text, &ParseOptions::default()).map_err(|error| {
        crate::config::fs::unreadable(path, format!("cannot parse the file: {error}"))
    })?;
    let document = root
        .object_value()
        .ok_or_else(|| structure_error(path, "the root value is not an object"))?;

    let Some(providers_property) = document.get("providers") else {
        return Ok(crate::harness::StatusSnapshot::default());
    };
    let providers = providers_property
        .value()
        .and_then(|value| value.as_object())
        .ok_or_else(|| structure_error(path, "`providers` is not an object"))?;

    if providers.get(PROVIDER_ID).is_some() {
        providers
            .object_value(PROVIDER_ID)
            .ok_or_else(|| structure_error(path, "`providers.tokencamp` is not an object"))?;
    } else {
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
            "providers.tokencamp",
            "models.json no longer exists",
        ));
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(Removal::nothing(
            "providers.tokencamp",
            "models.json is empty",
        ));
    }

    let text = std::str::from_utf8(contents).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    let root = CstRootNode::parse(text, &ParseOptions::default()).map_err(|error| {
        crate::config::fs::unreadable(path, format!("cannot parse the file: {error}"))
    })?;
    let document = root
        .object_value()
        .ok_or_else(|| structure_error(path, "the root value is not an object"))?;
    let Some(providers_property) = document.get("providers") else {
        return Ok(Removal::nothing(
            "providers.tokencamp",
            "there is no tokencamp provider entry",
        ));
    };
    let providers = providers_property
        .value()
        .and_then(|value| value.as_object())
        .ok_or_else(|| structure_error(path, "`providers` is not an object"))?;
    let Some(provider) = providers.get(PROVIDER_ID) else {
        return Ok(Removal::nothing(
            "providers.tokencamp",
            "there is no tokencamp provider entry",
        ));
    };

    let owned = entry
        .and_then(|entry| entry.provider_entry.as_deref())
        .is_some_and(|provider| provider == PROVIDER_ID);
    let hash = crate::config::manifest::hash_bytes(contents);
    let modified =
        entry.is_some_and(|entry| entry.last_written_hash.as_deref() != Some(hash.as_str()));
    let adopted = entry.is_some_and(|entry| entry.adopted);

    if !owned && !force {
        return Ok(Removal::keep(
            "providers.tokencamp",
            "config_conflict",
            "the Pi provider entry `tokencamp` is not recorded as managed by tcamp; re-run with `--force` to remove it",
        ));
    }
    if owned && adopted && !force {
        return Ok(Removal::keep(
            "providers.tokencamp",
            "config_conflict",
            "the Pi provider entry was adopted from an existing entry; re-run with `--force` to remove it",
        ));
    }
    if owned && modified && !force {
        return Ok(Removal::keep(
            "providers.tokencamp",
            "config_drift",
            "models.json was modified outside tcamp; re-run with `--force` to remove the provider entry",
        ));
    }

    provider.remove();
    if providers.properties().is_empty() {
        if let Some(property) = document.get("providers") {
            property.remove();
        }
    }

    let meaningful = document
        .properties()
        .iter()
        .any(|property| property_name(property).as_deref() != Some("$schema"));
    if !meaningful && entry.is_some_and(|entry| entry.created_file) && !existing.resolved.symlink {
        return Ok(Removal::delete("models.json"));
    }

    let mut text = root.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(Removal::rewrite("providers.tokencamp", text.into_bytes()))
}

pub fn plan_removal_settings(
    path: &Path,
    existing: &crate::config::TargetContents,
    entry: Option<&ManifestEntry>,
    force: bool,
) -> Result<crate::uninstall::Removal, TcampError> {
    use crate::uninstall::Removal;

    let Some(contents) = existing.contents.as_deref() else {
        return Ok(Removal::nothing(
            "defaultProvider and defaultModel",
            "settings.json no longer exists",
        ));
    };

    let text = std::str::from_utf8(contents).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    let root = CstRootNode::parse(text, &strict_options()).map_err(|error| {
        crate::config::fs::unreadable(path, format!("cannot parse the file: {error}"))
    })?;
    let document = root
        .object_value()
        .ok_or_else(|| structure_error(path, "the root value is not an object"))?;

    let (defaults, created_file) = match entry {
        Some(entry) => {
            if entry.defaults.is_empty() {
                return Ok(Removal::nothing(
                    "defaultProvider and defaultModel",
                    "no tcamp-managed defaults are recorded",
                ));
            }
            let hash = crate::config::manifest::hash_bytes(contents);
            let modified = entry.last_written_hash.as_deref() != Some(hash.as_str());
            if modified && !force {
                return Ok(Removal::keep(
                    "defaultProvider and defaultModel",
                    "config_drift",
                    "settings.json was modified outside tcamp; re-run with `--force` to remove the tcamp defaults",
                ));
            }
            (entry.defaults.clone(), entry.created_file)
        }
        None if force => {
            let mut defaults = BTreeMap::new();
            if string_property(&document, "defaultProvider").as_deref() == Some("tokencamp") {
                defaults.insert("defaultProvider".to_string(), "tokencamp".to_string());
            }
            if let Some(model) = string_property(&document, "defaultModel") {
                if crate::models::catalog_model(&model).is_some() {
                    defaults.insert("defaultModel".to_string(), model);
                }
            }
            if defaults.is_empty() {
                return Ok(Removal::nothing(
                    "defaultProvider and defaultModel",
                    "no tcamp defaults were found in settings.json",
                ));
            }
            (defaults, false)
        }
        None => {
            return Ok(Removal::nothing(
                "defaultProvider and defaultModel",
                "no tcamp-managed defaults are recorded",
            ));
        }
    };

    let mut removed = false;
    for name in defaults.keys() {
        if let Some(property) = document.get(name) {
            property.remove();
            removed = true;
        }
    }
    if !removed {
        return Ok(Removal::nothing(
            "defaultProvider and defaultModel",
            "the tcamp defaults are already gone",
        ));
    }

    if document.properties().is_empty() && created_file && !existing.resolved.symlink {
        return Ok(Removal::delete("settings.json"));
    }

    let mut text = root.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(Removal::rewrite(
        "defaultProvider and defaultModel",
        text.into_bytes(),
    ))
}

pub fn inspect_models(
    existing: Option<&[u8]>,
    path: &Path,
    entry: Option<&ManifestEntry>,
    force: bool,
) -> Result<Inspection, TcampError> {
    let inspection = |drift: bool, adopted: bool, model_ids: Vec<String>| Inspection {
        drift,
        adopted,
        model_ids,
        provider_entry: Some(PROVIDER_ID.to_string()),
        defaults: BTreeMap::new(),
    };

    let Some(contents) = existing else {
        return Ok(inspection(false, false, managed_model_ids()));
    };
    if contents.iter().all(u8::is_ascii_whitespace) {
        return Ok(inspection(false, false, managed_model_ids()));
    }

    let text = std::str::from_utf8(contents).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    let root = CstRootNode::parse(text, &ParseOptions::default()).map_err(|error| {
        crate::config::fs::unreadable(path, format!("cannot parse the file: {error}"))
    })?;
    let document = root
        .object_value()
        .ok_or_else(|| structure_error(path, "the root value is not an object"))?;

    let Some(providers_property) = document.get("providers") else {
        return Ok(inspection(false, false, managed_model_ids()));
    };
    let providers = providers_property
        .value()
        .and_then(|value| value.as_object())
        .ok_or_else(|| structure_error(path, "`providers` is not an object"))?;

    let Some(provider) = providers.object_value(PROVIDER_ID) else {
        if providers.get(PROVIDER_ID).is_some() {
            return Err(structure_error(
                path,
                "`providers.tokencamp` is not an object",
            ));
        }
        return Ok(inspection(false, false, managed_model_ids()));
    };

    let owned = entry
        .and_then(|entry| entry.provider_entry.as_deref())
        .is_some_and(|provider| provider == PROVIDER_ID);
    if !owned && !force {
        return Err(unowned_conflict(path));
    }

    let drift = entry
        .and_then(|entry| entry.last_written_hash.as_deref())
        .is_some_and(|previous| previous != crate::config::manifest::hash_bytes(contents));
    let model_ids = if owned {
        entry
            .map(|entry| entry.model_ids.clone())
            .unwrap_or_else(managed_model_ids)
    } else {
        adopted_model_ids(&provider)
    };
    Ok(inspection(drift, !owned, model_ids))
}

fn adopted_model_ids(provider: &CstObject) -> Vec<String> {
    let existing: Vec<&str> = provider
        .get("models")
        .and_then(|property| property.value())
        .and_then(|value| value.as_array())
        .map(|models| {
            exposed_ids()
                .into_iter()
                .filter(|id| find_model(&models, id).is_some())
                .collect()
        })
        .unwrap_or_default();
    exposed_ids()
        .into_iter()
        .filter(|id| !existing.contains(id))
        .map(str::to_string)
        .collect()
}

fn string_property(object: &CstObject, name: &str) -> Option<String> {
    value_property(object, name)?.as_str().map(str::to_string)
}

fn value_property(object: &CstObject, name: &str) -> Option<Value> {
    node_to_serde(&object.get(name)?.value()?)
}

pub fn unowned_conflict(path: &Path) -> TcampError {
    TcampError::config_conflict(
        format!(
            "the Pi provider entry `tokencamp` in {} is not managed by tcamp",
            path.display()
        ),
        "re-run with `--force` to back up the file and adopt the entry; tcamp never overwrites an unowned provider entry without confirmation",
    )
}

#[derive(Debug, Default)]
pub struct SettingsMerge {
    pub contents: Option<Vec<u8>>,
    pub defaults: BTreeMap<String, String>,
}

pub fn merge_settings(existing: Option<&[u8]>, path: &Path) -> Result<SettingsMerge, TcampError> {
    let created = match existing {
        None => true,
        Some(contents) => std::str::from_utf8(contents)
            .map_err(|error| {
                crate::config::fs::unreadable(
                    path,
                    format!("the file is not valid UTF-8 ({error})"),
                )
            })?
            .trim()
            .is_empty(),
    };

    let root = if created {
        CstRootNode::parse("{}", &strict_options()).expect("an empty JSON object is valid")
    } else {
        let text =
            std::str::from_utf8(existing.expect("existing contents")).expect("checked above");
        CstRootNode::parse(text, &strict_options()).map_err(|error| {
            crate::config::fs::unreadable(path, format!("cannot parse the file: {error}"))
        })?
    };
    let document = root
        .object_value()
        .ok_or_else(|| structure_error(path, "the root value is not an object"))?;

    let mut defaults = BTreeMap::new();
    for (name, value) in [
        ("defaultProvider", "tokencamp"),
        ("defaultModel", models::DEFAULT_MAIN_MODEL),
    ] {
        let empty = match document.get(name) {
            None => true,
            Some(property) => match property.value().and_then(|value| node_to_serde(&value)) {
                None | Some(Value::Null) => true,
                Some(Value::String(value)) => value.trim().is_empty(),
                Some(_) => false,
            },
        };
        if empty {
            set_property(&document, name, CstInputValue::from(value));
            defaults.insert(name.to_string(), value.to_string());
        }
    }

    if defaults.is_empty() {
        return Ok(SettingsMerge {
            contents: None,
            defaults,
        });
    }

    let mut text = root.to_string();
    if created && !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(SettingsMerge {
        contents: Some(text.into_bytes()),
        defaults,
    })
}

pub fn inspect_settings(existing: Option<&[u8]>, path: &Path) -> Result<Inspection, TcampError> {
    let merge = merge_settings(existing, path)?;
    Ok(Inspection {
        drift: false,
        adopted: false,
        model_ids: Vec::new(),
        provider_entry: None,
        defaults: merge.defaults,
    })
}

fn strict_options() -> ParseOptions {
    ParseOptions {
        allow_comments: false,
        allow_loose_object_property_names: false,
        allow_trailing_commas: false,
    }
}

fn property_name(property: &CstObjectProp) -> Option<String> {
    property.name()?.decoded_value().ok()
}

fn node_to_serde(node: &CstNode) -> Option<Value> {
    match node {
        CstNode::Container(CstContainerNode::Object(object)) => {
            let mut map = serde_json::Map::new();
            for property in object.properties() {
                let Some(name) = property_name(&property) else {
                    continue;
                };
                let Some(value) = property.value().and_then(|value| node_to_serde(&value)) else {
                    continue;
                };
                map.insert(name, value);
            }
            Some(Value::Object(map))
        }
        CstNode::Container(CstContainerNode::Array(array)) => Some(Value::Array(
            array.elements().iter().filter_map(node_to_serde).collect(),
        )),
        CstNode::Leaf(CstLeafNode::BooleanLit(value)) => Some(Value::Bool(value.value())),
        CstNode::Leaf(CstLeafNode::NullKeyword(_)) => Some(Value::Null),
        CstNode::Leaf(CstLeafNode::NumberLit(value)) => value.to_string().parse().ok(),
        CstNode::Leaf(CstLeafNode::StringLit(value)) => {
            value.decoded_value().ok().map(Value::String)
        }
        // JSONC values are containers or these literal leaves; root, property,
        // token, whitespace, comment, and word-literal nodes cannot appear in a
        // value position.
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ShadowingConflict {
    pub path: PathBuf,
}

impl ShadowingConflict {
    pub fn message(&self) -> String {
        format!(
            "Pi auth.json {} has a top-level `tokencamp` entry, which would shadow the API key injected by tcamp",
            self.path.display()
        )
    }
}

#[derive(Debug, Clone)]
pub struct ShadowingUnreadable {
    pub path: PathBuf,
    pub detail: String,
}

impl ShadowingUnreadable {
    pub fn message(&self) -> String {
        format!(
            "cannot inspect Pi auth.json {}: {}",
            self.path.display(),
            self.detail
        )
    }
}

#[derive(Debug, Default)]
pub struct ShadowingReport {
    pub conflicts: Vec<ShadowingConflict>,
    pub unreadable: Vec<ShadowingUnreadable>,
}

impl ShadowingReport {
    pub fn is_empty(&self) -> bool {
        self.conflicts.is_empty() && self.unreadable.is_empty()
    }

    pub fn warnings(&self) -> Vec<Warning> {
        let mut warnings: Vec<Warning> = self
            .unreadable
            .iter()
            .map(|problem| Warning::new("shadowing_unreadable", problem.message()))
            .collect();
        warnings.extend(
            self.conflicts
                .iter()
                .map(|conflict| Warning::new("shadowing_conflict", conflict.message())),
        );
        warnings
    }

    pub fn error(&self) -> TcampError {
        if let Some(conflict) = self.conflicts.first() {
            return TcampError::config_conflict(
                conflict.message(),
                format!(
                    "remove the `tokencamp` entry from {} (for example by logging out of that provider in `pi`) or switch to the Pi login you want to use; tcamp does not read or change auth.json values",
                    conflict.path.display()
                ),
            );
        }
        let message = self
            .unreadable
            .iter()
            .map(ShadowingUnreadable::message)
            .collect::<Vec<_>>()
            .join("; ");
        TcampError::config_unreadable(
            message,
            "fix or remove the Pi auth.json file and try again; tcamp cannot rule out credential shadowing",
        )
    }
}

pub fn inspect_shadowing() -> Result<ShadowingReport, TcampError> {
    let path = paths()?.auth;
    let mut report = ShadowingReport::default();

    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(error) => {
            report.unreadable.push(ShadowingUnreadable {
                path,
                detail: format!("cannot read the file: {error}"),
            });
            return Ok(report);
        }
    };
    let keys: AuthKeys = match serde_json::from_slice(&contents) {
        Ok(keys) => keys,
        Err(error) => {
            report.unreadable.push(ShadowingUnreadable {
                path,
                detail: format!(
                    "cannot parse it at line {}, column {}: {error}",
                    error.line(),
                    error.column()
                ),
            });
            return Ok(report);
        }
    };
    if keys.0.iter().any(|key| key == PROVIDER_ID) {
        report.conflicts.push(ShadowingConflict { path });
    }
    Ok(report)
}

#[derive(Debug, Default)]
struct AuthKeys(Vec<String>);

impl<'de> Deserialize<'de> for AuthKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct KeysVisitor;

        impl<'de> Visitor<'de> for KeysVisitor {
            type Value = AuthKeys;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut keys = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    keys.push(key);
                    map.next_value::<IgnoredAny>()?;
                }
                Ok(AuthKeys(keys))
            }
        }

        deserializer.deserialize_map(KeysVisitor)
    }
}

fn structure_error(path: &Path, reason: impl std::fmt::Display) -> TcampError {
    crate::config::fs::unreadable(path, reason)
}
