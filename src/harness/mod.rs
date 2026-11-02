use std::env;
use std::path::Path;

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
        .filter(|harness| binary_on_path(harness.binary()))
        .collect()
}

fn binary_on_path(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| is_executable(&dir.join(name)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
