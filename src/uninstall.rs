use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::UninstallArgs;
use crate::config::manifest::Manifest;
use crate::config::{self, backup, fs as config_fs, lock};
use crate::errors::TcampError;
use crate::harness::{Harness, claude, kimi, pi};
use crate::output;
use crate::terminal;
use crate::warnings::Warning;

pub const CREDENTIALS_COMMAND: &str = "tcamp logout --all";
pub const BINARY_COMMAND: &str = "rm \"$(command -v tcamp)\"";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Delete,
    Rewrite,
    Keep,
    None,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Rewrite => "remove",
            Self::Keep => "keep",
            Self::None => "none",
        }
    }

    fn writes(self) -> bool {
        matches!(self, Self::Delete | Self::Rewrite)
    }
}

#[derive(Debug, Clone)]
pub struct Removal {
    pub action: Action,
    pub detail: String,
    pub reason: Option<String>,
    pub warning_code: Option<&'static str>,
    pub contents: Option<Vec<u8>>,
    pub remove_entry: bool,
}

impl Removal {
    pub fn delete(detail: impl Into<String>) -> Self {
        Self {
            action: Action::Delete,
            detail: detail.into(),
            reason: None,
            warning_code: None,
            contents: None,
            remove_entry: true,
        }
    }

    pub fn rewrite(detail: impl Into<String>, contents: Vec<u8>) -> Self {
        Self {
            action: Action::Rewrite,
            detail: detail.into(),
            reason: None,
            warning_code: None,
            contents: Some(contents),
            remove_entry: true,
        }
    }

    pub fn keep(
        detail: impl Into<String>,
        warning_code: &'static str,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            action: Action::Keep,
            detail: detail.into(),
            reason: Some(reason.into()),
            warning_code: Some(warning_code),
            contents: None,
            remove_entry: false,
        }
    }

    pub fn nothing(detail: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            action: Action::None,
            detail: detail.into(),
            reason: Some(reason.into()),
            warning_code: None,
            contents: None,
            remove_entry: false,
        }
    }
}

#[derive(Debug)]
struct TargetPlan {
    harness: Harness,
    path: PathBuf,
    resolved: PathBuf,
    key: String,
    action: Action,
    detail: String,
    reason: Option<String>,
    expected: Option<Vec<u8>>,
    desired: Option<Vec<u8>>,
    remove_entry: bool,
    entry_backups: Vec<String>,
}

#[derive(Debug)]
struct HarnessPlan {
    harness: Harness,
    targets: Vec<TargetPlan>,
    failures: Vec<(PathBuf, TcampError)>,
}

pub fn run(args: UninstallArgs, json: bool) -> Result<(), TcampError> {
    let harnesses = select_harnesses(&args.harness)?;
    let mut manifest = Manifest::load()?;

    let mut warnings = Vec::new();
    let mut plans = Vec::new();
    for harness in &harnesses {
        plans.push(plan_harness(
            *harness,
            &manifest,
            args.force,
            &mut warnings,
        )?);
    }
    let mut backups: BTreeSet<String> = BTreeSet::new();
    for harness in &harnesses {
        for path in backup::list_all(harness.id()).unwrap_or_default() {
            backups.insert(path.display().to_string());
        }
    }
    for plan in &plans {
        for target in &plan.targets {
            backups.extend(target.entry_backups.iter().cloned());
        }
    }
    let credentials = crate::credentials::credentials_path()?;
    let has_writes = plans
        .iter()
        .any(|plan| plan.targets.iter().any(|target| target.action.writes()));

    if !json {
        print_plan(&plans);
        for warning in &warnings {
            eprintln!("warning: {} [{}]", warning.message, warning.code);
        }
    }

    if !args.dry_run && !args.yes {
        if json || !terminal::is_tty() {
            return Err(TcampError::config_conflict(
                "cannot confirm the uninstall plan without an interactive terminal",
                "re-run with `--yes` to apply the plan, or use `--dry-run` to print it",
            ));
        }
        match terminal::confirm("Proceed with uninstall? [y/N] ") {
            Ok(Some(true)) => {}
            Ok(Some(false)) | Ok(None) => {
                println!("No changes.");
                return Ok(());
            }
            Err(error) => {
                return Err(TcampError::io(
                    format!("failed to read the confirmation: {error}"),
                    "re-run with `--yes` to apply the plan without a prompt",
                ));
            }
        }
    }

    let mut failures: Vec<(PathBuf, TcampError)> = Vec::new();
    for plan in &mut plans {
        failures.append(&mut plan.failures);
    }

    if args.dry_run {
        if !json {
            print_notes(&credentials, &backups);
        }
        if !failures.is_empty() {
            return Err(failures.remove(0).1);
        }
        if json {
            output::print_json(&Report {
                ok: true,
                dry_run: true,
                harnesses: harness_reports(&plans),
                credentials: CredentialsReport {
                    path: credentials.display().to_string(),
                    retained: true,
                    command: CREDENTIALS_COMMAND,
                },
                backups: backups.into_iter().collect(),
                binary: BinaryReport {
                    removed: false,
                    command: BINARY_COMMAND,
                },
                warnings,
            })?;
        } else {
            eprintln!("no files written");
        }
        return Ok(());
    }

    let mut manifest_dirty = false;
    for plan in &plans {
        for target in &plan.targets {
            if target.action.writes() {
                match apply_target(target) {
                    Ok(backup) => {
                        if let Some(backup) = backup {
                            backups.insert(backup.display().to_string());
                        }
                        manifest.targets.remove(&target.key);
                        manifest_dirty = true;
                    }
                    Err(error) => failures.push((target.path.clone(), error)),
                }
            } else if target.remove_entry {
                manifest.targets.remove(&target.key);
                manifest_dirty = true;
            }
        }
    }

    if manifest_dirty {
        if let Err(error) = manifest.save() {
            failures.push((manifest.path.clone(), error));
        }
    }

    if !json {
        print_notes(&credentials, &backups);
    }

    if !failures.is_empty() {
        return report_failures(&failures, json);
    }

    if !json && has_writes {
        println!("Uninstalled the managed configuration.");
    }

    if json {
        output::print_json(&Report {
            ok: true,
            dry_run: false,
            harnesses: harness_reports(&plans),
            credentials: CredentialsReport {
                path: credentials.display().to_string(),
                retained: true,
                command: CREDENTIALS_COMMAND,
            },
            backups: backups.into_iter().collect(),
            binary: BinaryReport {
                removed: false,
                command: BINARY_COMMAND,
            },
            warnings,
        })?;
    }

    Ok(())
}

fn report_failures(failures: &[(PathBuf, TcampError)], json: bool) -> Result<(), TcampError> {
    let message = format!(
        "uninstall failed for {} config target(s): {}",
        failures.len(),
        failures
            .iter()
            .map(|(path, error)| format!("{} ({error})", path.display()))
            .collect::<Vec<_>>()
            .join("; ")
    );
    if !json {
        for (path, error) in failures {
            eprintln!(
                "error: failed to remove {}: {} [{}]",
                path.display(),
                error,
                error.code()
            );
        }
    }
    Err(TcampError::io(
        message,
        "fix the reported paths and re-run `tcamp uninstall`; completed removals are kept",
    ))
}

fn select_harnesses(names: &[String]) -> Result<Vec<Harness>, TcampError> {
    if names.is_empty() {
        return Ok(Harness::ALL.to_vec());
    }
    let mut selected = Vec::new();
    for name in names {
        let harness = Harness::from_id(name).ok_or_else(|| {
            TcampError::usage(
                format!("{name:?} is not a harness managed by tcamp"),
                format!(
                    "use one or more of: {}",
                    Harness::ALL.map(Harness::id).join(", ")
                ),
            )
        })?;
        if !selected.contains(&harness) {
            selected.push(harness);
        }
    }
    Ok(selected)
}

fn plan_harness(
    harness: Harness,
    manifest: &Manifest,
    force: bool,
    warnings: &mut Vec<Warning>,
) -> Result<HarnessPlan, TcampError> {
    let mut paths = primary_targets(harness)?;
    for (path, entry) in &manifest.targets {
        if entry.harness == harness.id() {
            let path = PathBuf::from(path);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    let mut targets = Vec::new();
    let mut failures = Vec::new();
    for path in paths {
        let existing = match config::read_target(&path) {
            Ok(existing) => existing,
            Err(error) => {
                failures.push((path, error));
                continue;
            }
        };
        let key = existing.resolved.path.display().to_string();
        let entry = manifest
            .targets
            .get(&key)
            .or_else(|| manifest.targets.get(&path.display().to_string()));

        let removal = match harness {
            Harness::Claude => claude::plan_removal(&path, &existing, entry, force),
            Harness::Pi if path.file_name().is_some_and(|name| name == "settings.json") => {
                pi::plan_removal_settings(&path, &existing, entry, force)
            }
            Harness::Pi => pi::plan_removal(&path, &existing, entry, force),
            Harness::Kimi => kimi::plan_removal(&path, &existing, entry, force),
        };
        let removal = match removal {
            Ok(removal) => removal,
            Err(error) => {
                failures.push((path, error));
                continue;
            }
        };

        if removal.action == Action::None && entry.is_none() {
            continue;
        }
        if let (Some(code), Some(reason)) = (removal.warning_code, &removal.reason) {
            warnings.push(Warning::new(code, reason.clone()));
        }
        let remove_entry = removal.remove_entry
            || (removal.action == Action::None && entry.is_some())
            || (existing.contents.is_none() && entry.is_some());
        targets.push(TargetPlan {
            harness,
            path: path.clone(),
            resolved: existing.resolved.path.clone(),
            key,
            action: removal.action,
            detail: removal.detail,
            reason: removal.reason,
            expected: existing.contents.clone(),
            desired: removal.contents,
            remove_entry,
            entry_backups: entry.map(|entry| entry.backups.clone()).unwrap_or_default(),
        });
    }
    targets.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(HarnessPlan {
        harness,
        targets,
        failures,
    })
}

fn primary_targets(harness: Harness) -> Result<Vec<PathBuf>, TcampError> {
    match harness {
        Harness::Claude => Ok(vec![config::claude_overlay_path()?]),
        Harness::Pi => {
            let paths = pi::paths()?;
            Ok(vec![paths.models, paths.settings])
        }
        Harness::Kimi => Ok(vec![kimi::paths()?.config]),
    }
}

fn apply_target(plan: &TargetPlan) -> Result<Option<PathBuf>, TcampError> {
    let _lock = lock::acquire(plan.harness.id(), &plan.resolved)?;
    let current = config::read_target(&plan.path)?;
    if current.contents != plan.expected {
        return Err(TcampError::io(
            format!(
                "the config target {} changed while tcamp was preparing the uninstall",
                plan.path.display()
            ),
            "close other tools that write this file and try again",
        ));
    }

    let backup = match current.contents.as_deref() {
        Some(contents) => Some(backup::backup_target(
            plan.harness.id(),
            &plan.path,
            contents,
        )?),
        None => None,
    };

    match plan.action {
        Action::Delete => {
            if current.resolved.symlink {
                return Err(TcampError::io(
                    format!(
                        "refusing to delete the symlinked config target {}",
                        plan.path.display()
                    ),
                    "remove the entries manually or replace the symlink with a regular file",
                ));
            }
            fs::remove_file(&current.resolved.path).map_err(|error| {
                TcampError::io(
                    format!(
                        "failed to remove the config target {}: {error}",
                        current.resolved.path.display()
                    ),
                    "check the file permissions and try again",
                )
            })?;
        }
        Action::Rewrite => {
            let desired = plan.desired.as_deref().ok_or_else(|| {
                TcampError::io(
                    format!(
                        "no replacement contents were prepared for {}",
                        plan.path.display()
                    ),
                    "re-run `tcamp uninstall`",
                )
            })?;
            config_fs::atomic_write(&current.resolved.path, desired, current.mode)?;
        }
        Action::Keep | Action::None => {}
    }

    Ok(backup)
}

fn print_plan(plans: &[HarnessPlan]) {
    println!("Uninstall plan:");
    let targets: Vec<&TargetPlan> = plans.iter().flat_map(|plan| plan.targets.iter()).collect();
    let has_failures = plans.iter().any(|plan| !plan.failures.is_empty());
    if targets.is_empty() && !has_failures {
        println!("No managed configuration to remove.");
    } else {
        for plan in plans {
            if plan.targets.is_empty() && plan.failures.is_empty() {
                println!("  {}: no managed configuration", plan.harness.id());
                continue;
            }
            println!("  {}", plan.harness.id());
            for target in &plan.targets {
                match target.action {
                    Action::Delete => println!("    delete {}", target.path.display()),
                    Action::Rewrite => println!(
                        "    remove {} from {}",
                        target.detail,
                        target.path.display()
                    ),
                    Action::Keep => println!(
                        "    keep {}: {}",
                        target.path.display(),
                        target.reason.as_deref().unwrap_or("kept")
                    ),
                    Action::None => println!(
                        "    nothing to do for {} ({})",
                        target.path.display(),
                        target.reason.as_deref().unwrap_or("no managed entries")
                    ),
                }
            }
            for (path, error) in &plan.failures {
                println!("    error {}: {}", path.display(), error);
            }
        }
    }
}

fn print_notes(credentials: &Path, backups: &BTreeSet<String>) {
    println!();
    println!(
        "Credentials: {} (kept; remove with `{CREDENTIALS_COMMAND}`)",
        credentials.display()
    );
    if backups.is_empty() {
        println!("Backups: none");
    } else {
        for backup in backups {
            println!("Backups: {backup} (kept)");
        }
    }
    println!("Binary: not removed; delete it with `{BINARY_COMMAND}`");
}

fn harness_reports(plans: &[HarnessPlan]) -> BTreeMap<String, HarnessReport> {
    plans
        .iter()
        .map(|plan| {
            (
                plan.harness.id().to_string(),
                HarnessReport {
                    targets: plan
                        .targets
                        .iter()
                        .map(|target| TargetReport {
                            path: target.path.display().to_string(),
                            action: target.action.as_str(),
                            detail: target.detail.clone(),
                            reason: target.reason.clone(),
                        })
                        .collect(),
                },
            )
        })
        .collect()
}

#[derive(Serialize)]
struct Report {
    ok: bool,
    dry_run: bool,
    harnesses: BTreeMap<String, HarnessReport>,
    credentials: CredentialsReport,
    backups: Vec<String>,
    binary: BinaryReport,
    warnings: Vec<Warning>,
}

#[derive(Serialize)]
struct HarnessReport {
    targets: Vec<TargetReport>,
}

#[derive(Serialize)]
struct TargetReport {
    path: String,
    action: &'static str,
    detail: String,
    reason: Option<String>,
}

#[derive(Serialize)]
struct CredentialsReport {
    path: String,
    retained: bool,
    command: &'static str,
}

#[derive(Serialize)]
struct BinaryReport {
    removed: bool,
    command: &'static str,
}
