pub mod claude;
pub mod kimi;
pub mod launch;
pub mod pi;
pub mod preflight;

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

pub fn api_base(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let root = match trimmed.strip_suffix("/v1") {
        Some(candidate) if candidate.contains("://") && !candidate.ends_with("://") => candidate,
        _ => trimmed,
    };
    format!("{root}/v1")
}
