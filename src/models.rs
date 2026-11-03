use crate::errors::TcampError;

pub const DEFAULT_MAIN_MODEL: &str = "zhipu/glm-5.3";
pub const OPUS_MODEL: &str = "deepseek/deepseek-v4-pro";
pub const SONNET_MODEL: &str = "zhipu/glm-5.3";
pub const HAIKU_MODEL: &str = "zhipu/glm-5.3-flash";
pub const FABLE_MODEL: &str = "deepseek/deepseek-v4-pro";
pub const KIMI_MODEL: &str = "kimi/kimi-k2.7-code";

pub const CLAUDE_SLOTS: [&str; 4] = [OPUS_MODEL, SONNET_MODEL, HAIKU_MODEL, FABLE_MODEL];

pub const CLAUDE_SELECTABLE_MODELS: [&str; 4] =
    [DEFAULT_MAIN_MODEL, OPUS_MODEL, HAIKU_MODEL, KIMI_MODEL];

pub const CLAUDE_MAX_OUTPUT_TOKENS: u64 = 32768;
pub const UNKNOWN_CONTEXT_TOKENS: u64 = 200_000;

pub const GLM_CONTEXT_TOKENS: u64 = 1_048_576;
pub const KIMI_CONTEXT_TOKENS: u64 = 262_144;
pub const AUTO_CONTEXT_TOKENS: u64 = 200_000;

pub const GLM_MAX_OUTPUT_TOKENS: u64 = 32768;
pub const AUTO_MAX_OUTPUT_TOKENS: u64 = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogModel {
    pub id: &'static str,
    pub context_window: u64,
    pub max_output: u64,
}

pub const EXPOSED_MODELS: [CatalogModel; 5] = [
    CatalogModel {
        id: DEFAULT_MAIN_MODEL,
        context_window: GLM_CONTEXT_TOKENS,
        max_output: GLM_MAX_OUTPUT_TOKENS,
    },
    CatalogModel {
        id: OPUS_MODEL,
        context_window: GLM_CONTEXT_TOKENS,
        max_output: GLM_MAX_OUTPUT_TOKENS,
    },
    CatalogModel {
        id: HAIKU_MODEL,
        context_window: GLM_CONTEXT_TOKENS,
        max_output: GLM_MAX_OUTPUT_TOKENS,
    },
    CatalogModel {
        id: KIMI_MODEL,
        context_window: KIMI_CONTEXT_TOKENS,
        max_output: GLM_MAX_OUTPUT_TOKENS,
    },
    CatalogModel {
        id: "tokencamp/auto",
        context_window: AUTO_CONTEXT_TOKENS,
        max_output: AUTO_MAX_OUTPUT_TOKENS,
    },
];

pub fn catalog_model(id: &str) -> Option<&'static CatalogModel> {
    EXPOSED_MODELS.iter().find(|model| model.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeModelSelection {
    pub main: String,
    pub overridden: bool,
}

pub fn resolve_claude_model(explicit: Option<&str>) -> Result<ClaudeModelSelection, TcampError> {
    let explicit = explicit.map(str::trim).map(str::to_string);
    let from_env = std::env::var("TOKENCAMP_MODEL")
        .ok()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty());

    match explicit.or(from_env) {
        None => Ok(ClaudeModelSelection {
            main: DEFAULT_MAIN_MODEL.to_string(),
            overridden: false,
        }),
        Some(model) => {
            if !CLAUDE_SELECTABLE_MODELS.contains(&model.as_str()) {
                return Err(TcampError::usage(
                    format!("{model:?} is not a canonical Catalog model id for Claude Code"),
                    format!(
                        "use one of: {}; `auto`, `@preset/...`, and `~family-latest` are not accepted",
                        CLAUDE_SELECTABLE_MODELS.join(", ")
                    ),
                ));
            }
            Ok(ClaudeModelSelection {
                main: model,
                overridden: true,
            })
        }
    }
}

pub fn claude_context_tokens(main_model: &str) -> u64 {
    std::iter::once(main_model)
        .chain(CLAUDE_SLOTS)
        .map(|model| {
            catalog_model(model)
                .map(|model| model.context_window)
                .unwrap_or(UNKNOWN_CONTEXT_TOKENS)
        })
        .min()
        .unwrap_or(UNKNOWN_CONTEXT_TOKENS)
}

pub fn claude_model_ids(main_model: &str) -> Vec<String> {
    let mut ids = vec![main_model.to_string()];
    for slot in CLAUDE_SLOTS {
        if !ids.iter().any(|id| id == slot) {
            ids.push(slot.to_string());
        }
    }
    ids
}
