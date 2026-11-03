use std::path::{Path, PathBuf};

use crate::config::manifest::{self, Manifest, ManifestEntry};
use crate::config::{TargetContents, backup, diff, fs, lock};
use crate::errors::TcampError;
use crate::terminal;
use crate::warnings::Warning;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    None,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Inspection {
    pub drift: bool,
    pub adopted: bool,
    pub model_ids: Vec<String>,
    pub provider_entry: Option<String>,
}

pub type InspectFn<'a> =
    dyn Fn(&TargetContents, Option<&ManifestEntry>) -> Result<Inspection, TcampError> + 'a;

pub struct Request<'a> {
    pub harness: &'a str,
    pub target: PathBuf,
    pub desired: Vec<u8>,
    pub dry_run: bool,
    pub prompt: bool,
    pub inspect: &'a InspectFn<'a>,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub target: PathBuf,
    pub action: Action,
    pub backup: Option<PathBuf>,
    pub diff: Option<String>,
    pub warnings: Vec<Warning>,
}

pub fn converge(request: Request<'_>) -> Result<Outcome, TcampError> {
    let mut existing = fs::read_target(&request.target)?;

    if request.dry_run {
        inspect(&request, &existing, None)?;
        let action = action_for(&existing, &request.desired);
        let diff = if action == Action::None {
            String::new()
        } else {
            diff::unified_diff(
                existing.contents.as_deref(),
                &request.desired,
                &request.target,
            )
        };
        return Ok(Outcome {
            target: request.target,
            action,
            backup: None,
            diff: Some(diff),
            warnings: Vec::new(),
        });
    }

    if existing.contents.is_none() && request.prompt {
        eprintln!("Config target: {}", request.target.display());
        match terminal::confirm_default_yes("Write it? [Y/n] ") {
            Ok(Some(true)) => {}
            Ok(_) => {
                return Ok(Outcome {
                    target: request.target,
                    action: Action::None,
                    backup: None,
                    diff: None,
                    warnings: Vec::new(),
                });
            }
            Err(error) => {
                return Err(TcampError::io(
                    format!("failed to read the confirmation: {error}"),
                    "re-run with `--yes` to write without a prompt",
                ));
            }
        }
    }

    let _lock = lock::acquire(request.harness, &existing.resolved.path)?;
    let mut retried = false;
    loop {
        let latest = fs::read_target(&request.target)?;
        if latest.contents == existing.contents {
            existing = latest;
            break;
        }
        if retried {
            return Err(TcampError::io(
                format!(
                    "the config target {} changed while tcamp was preparing the update",
                    request.target.display()
                ),
                "close other tools that write this file and try again",
            ));
        }
        retried = true;
        existing = latest;
    }

    let mut manifest_store = Some(Manifest::load()?);
    let inspection = inspect(&request, &existing, manifest_store.as_ref())?;
    let action = action_for(&existing, &request.desired);
    let creating = existing.contents.is_none();

    let mut warnings = Vec::new();
    if inspection.drift {
        warnings.push(Warning::new(
            "config_drift",
            format!(
                "the config target {} was modified outside tcamp; managed values were rewritten",
                request.target.display()
            ),
        ));
    }

    let mut backup = None;
    match (creating, action) {
        (true, _) => {
            fs::atomic_write(&existing.resolved.path, &request.desired, None)?;
        }
        (false, Action::Update) => {
            let contents = existing
                .contents
                .as_deref()
                .expect("existing config target");
            backup = Some(backup::backup_target(
                request.harness,
                &request.target,
                contents,
            )?);
            fs::atomic_write(&existing.resolved.path, &request.desired, existing.mode)?;
        }
        _ => {}
    }

    if let Some(manifest_store) = manifest_store.as_mut() {
        let key = existing.resolved.path.display().to_string();
        let entry = manifest_store.targets.entry(key).or_default();
        entry.harness = request.harness.to_string();
        entry.created_file = entry.created_file || creating;
        entry.adopted = entry.adopted || inspection.adopted;
        if inspection.provider_entry.is_some() {
            entry.provider_entry = inspection.provider_entry.clone();
        }
        entry.model_ids = inspection.model_ids.clone();
        entry.last_written_hash = Some(manifest::hash_bytes(&request.desired));
        if backup.is_some() {
            entry.backups = backup::list_backups(request.harness, &request.target)?
                .iter()
                .map(|path| path.display().to_string())
                .collect();
        }
        entry.updated_at = Some(manifest::now());

        let manifest_path = manifest_store.path.clone();
        if let Err(error) = manifest_store.save() {
            return Err(rollback_manifest_failure(
                &existing,
                backup.as_deref(),
                &manifest_path,
                error,
            ));
        }
    }

    Ok(Outcome {
        target: request.target,
        action,
        backup,
        diff: None,
        warnings,
    })
}

fn inspect(
    request: &Request<'_>,
    existing: &TargetContents,
    manifest: Option<&Manifest>,
) -> Result<Inspection, TcampError> {
    let key = existing.resolved.path.display().to_string();
    let entry = manifest.and_then(|manifest| manifest.targets.get(&key));
    (request.inspect)(existing, entry)
}

fn action_for(existing: &TargetContents, desired: &[u8]) -> Action {
    match existing.contents.as_deref() {
        None => Action::Create,
        Some(contents) if contents == desired => Action::None,
        Some(_) => Action::Update,
    }
}

fn rollback_manifest_failure(
    target: &TargetContents,
    backup: Option<&Path>,
    manifest_path: &Path,
    failure: TcampError,
) -> TcampError {
    let created = target.contents.is_none();
    match rollback_target(&target.resolved.path, created, backup, target.mode) {
        Ok(()) => failure,
        Err(rollback_error) => TcampError::config_unreadable(
            format!(
                "failed to write manifest {}: {failure}; rolling back {} also failed: {rollback_error}",
                manifest_path.display(),
                target.resolved.path.display()
            ),
            format!(
                "restore {} manually from its backup, then fix or remove {}",
                target.resolved.path.display(),
                manifest_path.display()
            ),
        ),
    }
}

fn rollback_target(
    path: &Path,
    created: bool,
    backup: Option<&Path>,
    mode: Option<u32>,
) -> Result<(), String> {
    if cfg!(debug_assertions) && std::env::var_os("TCAMP_TEST_FAIL_ROLLBACK").is_some() {
        return Err("injected rollback failure".to_string());
    }

    if created {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    } else if let Some(backup) = backup {
        let contents = std::fs::read(backup).map_err(|error| error.to_string())?;
        fs::atomic_write(path, &contents, mode).map_err(|error| error.to_string())
    } else {
        Ok(())
    }
}
