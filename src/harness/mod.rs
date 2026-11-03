pub mod claude;
pub mod launch;
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
