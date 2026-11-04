use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::{self, manifest::Manifest};
use crate::credentials::ConnectionContext;
use crate::errors::TcampError;
use crate::harness::{self, Harness, StatusSnapshot, claude, kimi, pi};
use crate::warnings::Warning;

pub struct HarnessStatus {
    pub installed: bool,
    pub configured: bool,
    pub target: PathBuf,
    pub conflict: bool,
}

pub struct Snapshot {
    pub connection: ConnectionContext,
    pub harnesses: BTreeMap<&'static str, HarnessStatus>,
    pub warnings: Vec<Warning>,
}

pub fn snapshot() -> Result<Snapshot, TcampError> {
    let connection = crate::credentials::status()?;
    let mut warnings = connection.warnings.clone();

    let manifest = match Manifest::load() {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            warnings.push(Warning::new("config_unreadable", error.to_string()));
            None
        }
    };

    let mut harnesses = BTreeMap::new();
    for harness in Harness::ALL {
        let target = target_path(harness)?;
        let installed = harness::launch::binary_path(harness).is_some();
        let status = inspect(harness, &target, manifest.as_ref(), &mut warnings);
        harnesses.insert(
            harness.id(),
            HarnessStatus {
                installed,
                configured: status.configured,
                target,
                conflict: status.conflict,
            },
        );
    }

    Ok(Snapshot {
        connection,
        harnesses,
        warnings,
    })
}

fn target_path(harness: Harness) -> Result<PathBuf, TcampError> {
    match harness {
        Harness::Claude => config::claude_overlay_path(),
        Harness::Pi => Ok(pi::paths()?.models),
        Harness::Kimi => Ok(kimi::paths()?.config),
    }
}

fn inspect(
    harness: Harness,
    target: &Path,
    manifest: Option<&Manifest>,
    warnings: &mut Vec<Warning>,
) -> StatusSnapshot {
    let existing = match config::read_target(target) {
        Ok(existing) => existing,
        Err(error) => {
            warnings.push(Warning::new(error.code(), error.to_string()));
            return StatusSnapshot {
                configured: false,
                conflict: true,
            };
        }
    };
    let entry = manifest.and_then(|manifest| {
        manifest
            .targets
            .get(&existing.resolved.path.display().to_string())
    });

    let result = match harness {
        Harness::Claude => claude::inspect_status(existing.contents.as_deref(), target),
        Harness::Pi => pi::inspect_status(existing.contents.as_deref(), target, entry),
        Harness::Kimi => kimi::inspect_status(existing.contents.as_deref(), target, entry),
    };

    match result {
        Ok(status) => status,
        Err(error) => {
            warnings.push(Warning::new(error.code(), error.to_string()));
            StatusSnapshot {
                configured: false,
                conflict: true,
            }
        }
    }
}
