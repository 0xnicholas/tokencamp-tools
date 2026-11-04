pub mod claude;
pub mod kimi;
pub mod launch;
pub mod pi;
pub mod preflight;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub configured: bool,
    pub conflict: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    Claude,
    Pi,
    Kimi,
}

impl Harness {
    pub const ALL: [Self; 3] = [Self::Claude, Self::Pi, Self::Kimi];

    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::Kimi => "kimi",
        }
    }

    pub fn binary(self) -> &'static str {
        self.id()
    }

    pub fn min_version(self) -> &'static str {
        match self {
            Self::Claude => claude::MIN_VERSION,
            Self::Pi => pi::MIN_VERSION,
            Self::Kimi => kimi::MIN_VERSION,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|harness| harness.id() == id)
    }
}

pub fn detect() -> Vec<Harness> {
    Harness::ALL
        .into_iter()
        .filter(|harness| launch::binary_path(*harness).is_some())
        .collect()
}

pub fn target_path(harness: Harness) -> Result<std::path::PathBuf, crate::errors::TcampError> {
    match harness {
        Harness::Claude => crate::config::claude_overlay_path(),
        Harness::Pi => Ok(pi::paths()?.models),
        Harness::Kimi => Ok(kimi::paths()?.config),
    }
}

pub fn api_base(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let root = match trimmed.strip_suffix("/v1") {
        Some(candidate) if candidate.contains("://") && !candidate.ends_with("://") => candidate,
        _ => trimmed,
    };
    format!("{root}/v1")
}
