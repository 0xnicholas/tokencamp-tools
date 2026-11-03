use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::errors::TcampError;
use crate::models;

pub const HARNESS_ID: &str = "claude";
pub const OVERLAY_VERSION: i64 = 1;
const OVERLAY_SCHEMA: &str = "https://json.schemastore.org/claude-code-settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayOwnership {
    Owned,
    Unowned,
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
