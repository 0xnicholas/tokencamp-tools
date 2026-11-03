use serde::Serialize;
use serde_json::Value;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::TcampError;
use crate::models;
use crate::warnings::Warning;

pub const HARNESS_ID: &str = "claude";
pub const OVERLAY_VERSION: i64 = 1;
pub const MESSAGES_UNVERIFIED_HINT: &str = "/v1/messages capability unverified; requires an anthropic deployment; run `tcamp doctor --harness claude`";
const OVERLAY_SCHEMA: &str = "https://json.schemastore.org/claude-code-settings.json";

#[derive(Debug, Clone)]
pub struct ShadowingConflict {
    pub path: PathBuf,
    pub key: &'static str,
}

impl ShadowingConflict {
    pub fn message(&self) -> String {
        format!(
            "Claude settings {} set {}, which would shadow the tcamp launch credential",
            self.path.display(),
            self.key
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
            "cannot inspect Claude settings {} for shadowing: {}",
            self.path.display(),
            self.detail
        )
    }
}

#[derive(Debug, Default)]
pub struct ShadowingReport {
    pub conflicts: Vec<ShadowingConflict>,
    pub unreadable: Vec<ShadowingUnreadable>,
    pub warnings: Vec<Warning>,
}

pub fn inspect_shadowing() -> ShadowingReport {
    let mut report = ShadowingReport::default();

    if let Some(user_settings) = user_settings_path() {
        inspect_file(&user_settings, false, &mut report);
    }

    for directory in project_directories() {
        for name in ["settings.json", "settings.local.json"] {
            inspect_file(&directory.join(".claude").join(name), false, &mut report);
        }
    }

    if let Some(managed) = managed_settings_path() {
        inspect_file(&managed, true, &mut report);
    }

    let mut seen = std::collections::HashSet::new();
    report
        .conflicts
        .retain(|conflict| seen.insert((conflict.path.clone(), conflict.key)));
    report
        .unreadable
        .dedup_by(|left, right| left.path == right.path);
    report
}

fn user_settings_path() -> Option<PathBuf> {
    let directory = env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".claude")))?;
    Some(directory.join("settings.json"))
}

fn project_directories() -> Vec<PathBuf> {
    let Some(current) = env::current_dir().ok() else {
        return Vec::new();
    };
    let mut directories = vec![current.clone()];
    if let Some(root) = git_root(&current) {
        if root != current {
            directories.push(root);
        }
    }
    directories
}

fn git_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(directory) = current {
        if fs::symlink_metadata(directory.join(".git")).is_ok() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}

fn managed_settings_path() -> Option<PathBuf> {
    if let Some(overridden) = env::var_os("TCAMP_TEST_CLAUDE_MANAGED_SETTINGS") {
        if cfg!(debug_assertions) {
            return Some(PathBuf::from(overridden));
        }
    }

    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(
            "/Library/Application Support/ClaudeCode/managed-settings.json",
        ))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Some(PathBuf::from("/etc/claude-code/managed-settings.json"))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn inspect_file(path: &Path, managed: bool, report: &mut ShadowingReport) {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            return record_unreadable(report, managed, path, format!("cannot inspect it: {error}"));
        }
    };
    if !metadata.is_file() {
        return record_unreadable(
            report,
            managed,
            path,
            "the path is not a regular file".to_string(),
        );
    }

    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) => {
            return record_unreadable(report, managed, path, format!("cannot read it: {error}"));
        }
    };
    let value: Value = match serde_json::from_slice(&contents) {
        Ok(value) => value,
        Err(error) => {
            return record_unreadable(
                report,
                managed,
                path,
                format!(
                    "cannot parse it at line {}, column {}: {error}",
                    error.line(),
                    error.column()
                ),
            );
        }
    };

    for (pointer, key) in [
        ("/env/ANTHROPIC_AUTH_TOKEN", "env.ANTHROPIC_AUTH_TOKEN"),
        ("/apiKeyHelper", "apiKeyHelper"),
    ] {
        if value
            .pointer(pointer)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        {
            report.conflicts.push(ShadowingConflict {
                path: path.to_path_buf(),
                key,
            });
        }
    }
}

fn record_unreadable(report: &mut ShadowingReport, managed: bool, path: &Path, detail: String) {
    if managed {
        report.warnings.push(Warning::new(
            "shadowing_managed_unreadable",
            format!(
                "cannot check Claude managed settings {} for shadowing: {detail}; continuing without this check",
                path.display()
            ),
        ));
    } else {
        report.unreadable.push(ShadowingUnreadable {
            path: path.to_path_buf(),
            detail,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayOwnership {
    Owned,
    Unowned,
}

pub fn unowned_conflict(target: &Path) -> TcampError {
    TcampError::config_conflict(
        format!(
            "the config target {} is not managed by tcamp",
            target.display()
        ),
        "re-run with `--force` to back up the file and adopt it; tcamp never overwrites an unowned overlay without confirmation",
    )
}

pub fn overlay_ownership(contents: &[u8], path: &Path) -> Result<OverlayOwnership, TcampError> {
    let text = std::str::from_utf8(contents).map_err(|error| {
        crate::config::fs::unreadable(path, format!("the file is not valid UTF-8 ({error})"))
    })?;
    let value: Value = serde_json::from_str(text).map_err(|error| {
        crate::config::fs::unreadable(
            path,
            format!(
                "cannot parse the file at line {}, column {}: {error}",
                error.line(),
                error.column()
            ),
        )
    })?;

    let Some(marker) = value.get("_tcamp").and_then(Value::as_object) else {
        return Ok(OverlayOwnership::Unowned);
    };
    let version = marker.get("version").and_then(Value::as_i64);
    let harness = marker.get("harness").and_then(Value::as_str);
    if version == Some(OVERLAY_VERSION) && harness == Some(HARNESS_ID) {
        Ok(OverlayOwnership::Owned)
    } else {
        Ok(OverlayOwnership::Unowned)
    }
}

#[derive(Serialize)]
struct Overlay<'a> {
    #[serde(rename = "$schema")]
    schema: &'static str,
    #[serde(rename = "_tcamp")]
    tcamp: Marker,
    env: Env<'a>,
}

#[derive(Serialize)]
struct Marker {
    version: i64,
    harness: &'static str,
}

#[derive(Serialize)]
struct Env<'a> {
    #[serde(rename = "ANTHROPIC_BASE_URL")]
    anthropic_base_url: &'a str,
    #[serde(rename = "ANTHROPIC_API_KEY")]
    anthropic_api_key: &'static str,
    #[serde(rename = "ANTHROPIC_MODEL")]
    anthropic_model: &'a str,
    #[serde(rename = "ANTHROPIC_DEFAULT_OPUS_MODEL")]
    anthropic_default_opus_model: &'static str,
    #[serde(rename = "ANTHROPIC_DEFAULT_SONNET_MODEL")]
    anthropic_default_sonnet_model: &'static str,
    #[serde(rename = "ANTHROPIC_DEFAULT_HAIKU_MODEL")]
    anthropic_default_haiku_model: &'static str,
    #[serde(rename = "ANTHROPIC_DEFAULT_FABLE_MODEL")]
    anthropic_default_fable_model: &'static str,
    #[serde(rename = "CLAUDE_CODE_MAX_CONTEXT_TOKENS")]
    max_context_tokens: String,
    #[serde(rename = "CLAUDE_CODE_MAX_OUTPUT_TOKENS")]
    max_output_tokens: String,
    #[serde(rename = "ENABLE_TOOL_SEARCH")]
    enable_tool_search: &'static str,
    #[serde(rename = "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS")]
    disable_experimental_betas: &'static str,
    #[serde(rename = "MAX_THINKING_TOKENS")]
    max_thinking_tokens: &'static str,
}

pub fn desired_overlay(base_url: &str, main_model: &str) -> String {
    let overlay = Overlay {
        schema: OVERLAY_SCHEMA,
        tcamp: Marker {
            version: OVERLAY_VERSION,
            harness: HARNESS_ID,
        },
        env: Env {
            anthropic_base_url: base_url,
            anthropic_api_key: "",
            anthropic_model: main_model,
            anthropic_default_opus_model: models::OPUS_MODEL,
            anthropic_default_sonnet_model: models::SONNET_MODEL,
            anthropic_default_haiku_model: models::HAIKU_MODEL,
            anthropic_default_fable_model: models::FABLE_MODEL,
            max_context_tokens: models::claude_context_tokens(main_model).to_string(),
            max_output_tokens: models::CLAUDE_MAX_OUTPUT_TOKENS.to_string(),
            enable_tool_search: "false",
            disable_experimental_betas: "1",
            max_thinking_tokens: "0",
        },
    };

    let mut text = serde_json::to_string_pretty(&overlay).expect("the overlay is serializable");
    text.push('\n');
    text
}

pub fn launch_args(
    target: &Path,
    passthrough: &[OsString],
    selection: &models::ClaudeModelSelection,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--settings"),
        target.as_os_str().to_os_string(),
    ];
    if selection.overridden {
        args.push(OsString::from("--model"));
        args.push(OsString::from(&selection.main));
    }
    args.extend(passthrough.iter().cloned());
    args
}

pub fn launch_environment(
    api_key: &str,
    selection: &models::ClaudeModelSelection,
) -> Vec<(String, String)> {
    let mut environment = vec![
        ("TOKENCAMP_API_KEY".to_string(), api_key.to_string()),
        ("ANTHROPIC_AUTH_TOKEN".to_string(), api_key.to_string()),
        ("ANTHROPIC_API_KEY".to_string(), String::new()),
    ];
    if selection.overridden {
        environment.push(("ANTHROPIC_MODEL".to_string(), selection.main.clone()));
        environment.push((
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS".to_string(),
            models::claude_context_tokens(&selection.main).to_string(),
        ));
    }
    environment
}
