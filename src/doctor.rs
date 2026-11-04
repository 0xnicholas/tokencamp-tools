use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anstream::{AutoStream, ColorChoice};
use anstyle::{AnsiColor, Color, Style};
use serde::Serialize;

use crate::cli::DoctorArgs;
use crate::config::{
    self,
    manifest::{Manifest, ManifestEntry},
};
use crate::credentials::{self, BaseUrl, CredentialStore, DEFAULT_BASE_URL, HttpPolicy};
use crate::errors::TcampError;
use crate::harness::{self, Harness, claude, kimi, pi};
use crate::models;
use crate::output;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Pass,
    Warn,
    Fail,
    Skip,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Check {
    id: String,
    scope: &'static str,
    harness: Option<&'static str>,
    status: Status,
    message: String,
    hint: Option<String>,
    code: Option<String>,
    request_id: Option<String>,
}

impl Check {
    fn new(
        id: impl Into<String>,
        scope: &'static str,
        harness: Option<&'static str>,
        status: Status,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            scope,
            harness,
            status,
            message: message.into(),
            hint: None,
            code: None,
            request_id: None,
        }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

#[derive(Debug, Default, Serialize)]
struct Summary {
    pass: usize,
    warn: usize,
    fail: usize,
    skip: usize,
}

#[derive(Debug, Serialize)]
struct DoctorKey {
    masked: String,
    source: &'static str,
    verified: Option<bool>,
    verified_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct HarnessView {
    installed: Option<String>,
    configured: bool,
    target: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    ok: bool,
    base_url: String,
    key: Option<DoctorKey>,
    network: bool,
    verify: bool,
    harnesses: BTreeMap<String, HarnessView>,
    checks: Vec<Check>,
    warnings: Vec<String>,
    summary: Summary,
}

pub fn run(
    args: DoctorArgs,
    json: bool,
    verbose: bool,
    color: ColorChoice,
) -> Result<u8, TcampError> {
    if args.network || args.verify {
        return Err(TcampError::usage(
            "network checks are not implemented in this build",
            "run `tcamp doctor` without `--network` or `--verify` for the offline report",
        ));
    }

    let selected = select_harnesses(args.harness.as_deref())?;
    let report = build_report(&selected)?;

    if json {
        output::print_json(&report)?;
    } else {
        print_board(&report, &selected, verbose, color)?;
    }

    Ok(if report.ok { 0 } else { 1 })
}

fn select_harnesses(name: Option<&str>) -> Result<Vec<Harness>, TcampError> {
    match name {
        None => Ok(Harness::ALL.to_vec()),
        Some(name) => Harness::from_id(name)
            .map(|harness| vec![harness])
            .ok_or_else(|| {
                TcampError::usage(
                    format!("{name:?} is not a harness managed by tcamp"),
                    format!("use one of: {}", Harness::ALL.map(Harness::id).join(", ")),
                )
            }),
    }
}

fn build_report(selected: &[Harness]) -> Result<DoctorReport, TcampError> {
    let credentials_path = credentials::credentials_path()?;
    let env_key = credentials::env_key();
    let credential_file = inspect_credential_file(&credentials_path, env_key.is_some());
    let file_check = credential_file.check;
    let store = credential_file.store;

    let raw_base_url = resolve_raw_base_url(store.as_ref());
    let parsed_base_url = BaseUrl::parse(&raw_base_url);
    let base_url = parsed_base_url
        .as_ref()
        .map(|base| base.as_str().to_string())
        .unwrap_or_else(|_| raw_base_url.clone());

    let (key_check, key) = inspect_key(
        store.as_ref(),
        parsed_base_url.as_ref().ok(),
        env_key.as_deref(),
    );
    let base_check = inspect_base_url(parsed_base_url.as_ref());

    let manifest = Manifest::load();
    let mut checks = vec![file_check, key_check, base_check];
    checks.push(Check::new(
        "network.reachable",
        "network",
        None,
        Status::Skip,
        "offline; use --network",
    ));
    checks.push(Check::new(
        "network.auth",
        "network",
        None,
        Status::Skip,
        "offline; use --network",
    ));

    let mut harnesses = BTreeMap::new();
    for harness in selected {
        let (view, harness_checks) = inspect_harness(*harness, &manifest);
        harnesses.insert(harness.id().to_string(), view);
        checks.extend(harness_checks);
    }

    let summary = summarize(&checks);
    let warnings = checks
        .iter()
        .filter(|check| check.status == Status::Warn)
        .map(|check| check.id.clone())
        .collect();

    Ok(DoctorReport {
        ok: summary.fail == 0,
        base_url,
        key,
        network: false,
        verify: false,
        harnesses,
        checks,
        warnings,
        summary,
    })
}

fn resolve_raw_base_url(store: Option<&CredentialStore>) -> String {
    std::env::var("TOKENCAMP_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| store.and_then(|store| store.active.clone()))
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

fn inspect_base_url(parsed: Result<&BaseUrl, &credentials::BaseUrlError>) -> Check {
    match parsed {
        Ok(base) => match base.http_policy() {
            HttpPolicy::Secure => Check::new(
                "connection.base_url",
                "connection",
                None,
                Status::Pass,
                base.as_str(),
            ),
            HttpPolicy::InsecureLocal => Check::new(
                "connection.base_url",
                "connection",
                None,
                Status::Pass,
                format!("{base} (plain HTTP on a private network)"),
            ),
            HttpPolicy::InsecurePublic => Check::new(
                "connection.base_url",
                "connection",
                None,
                Status::Fail,
                format!("refusing to send an API key over plain HTTP to {base}"),
            )
            .with_hint("use https:// or a loopback, RFC1918, or link-local address")
            .with_code("usage"),
        },
        Err(error) => {
            let error = TcampError::from((*error).clone());
            Check::new(
                "connection.base_url",
                "connection",
                None,
                Status::Fail,
                error.to_string(),
            )
            .with_hint(error.hint())
            .with_code(error.code())
        }
    }
}

struct CredentialFile {
    check: Check,
    store: Option<CredentialStore>,
}

fn inspect_credential_file(path: &Path, env_key_present: bool) -> CredentialFile {
    let directory_mode = path.parent().and_then(directory_mode);

    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut message = format!("credentials file not found at {}", path.display());
            if env_key_present {
                message.push_str("; using TOKENCAMP_API_KEY");
            }
            if let Some(mode) = directory_mode.filter(|mode| *mode != 0o700) {
                message.push_str(&format!("; directory mode is {mode:04o}, expected 0700"));
            }
            let hint = match directory_mode.filter(|mode| *mode != 0o700) {
                Some(_) => format!(
                    "run `chmod 700 {}` and `tcamp login --key-stdin` to save a key",
                    path.parent()
                        .map(|parent| parent.display().to_string())
                        .unwrap_or_default()
                ),
                None => "run `tcamp login --key-stdin` to save a key".to_string(),
            };
            CredentialFile {
                check: Check::new("credential.file", "connection", None, Status::Warn, message)
                    .with_hint(hint),
                store: None,
            }
        }
        Err(error) => CredentialFile {
            check: Check::new(
                "credential.file",
                "connection",
                None,
                Status::Fail,
                format!(
                    "cannot inspect credentials file {}: {error}",
                    path.display()
                ),
            )
            .with_hint("check the credentials path and its permissions")
            .with_code("config_unreadable"),
            store: None,
        },
        Ok(metadata) if metadata.file_type().is_symlink() => CredentialFile {
            check: Check::new(
                "credential.file",
                "connection",
                None,
                Status::Fail,
                format!(
                    "credentials file {} is a symlink; tcamp refuses to follow it",
                    path.display()
                ),
            )
            .with_hint("replace the symlink with a regular file and re-run `tcamp login`")
            .with_code("config_unreadable"),
            store: None,
        },
        Ok(metadata) if !metadata.is_file() => CredentialFile {
            check: Check::new(
                "credential.file",
                "connection",
                None,
                Status::Fail,
                format!("credentials file {} is not a regular file", path.display()),
            )
            .with_hint("remove the path and re-run `tcamp login`")
            .with_code("config_unreadable"),
            store: None,
        },
        Ok(metadata) => match CredentialStore::load() {
            Err(error) => CredentialFile {
                check: Check::new(
                    "credential.file",
                    "connection",
                    None,
                    Status::Fail,
                    error.to_string(),
                )
                .with_hint(error.hint())
                .with_code(error.code()),
                store: None,
            },
            Ok(store) => {
                let mode = permission_mode(&metadata);
                let check = if mode.is_some_and(|mode| mode != 0o600) {
                    Check::new(
                        "credential.file",
                        "connection",
                        None,
                        Status::Fail,
                        format!(
                            "credentials file {} has mode {:04o}; expected 0600",
                            path.display(),
                            mode.expect("checked above")
                        ),
                    )
                    .with_hint(format!("run `chmod 600 {}`", path.display()))
                    .with_code("permissions")
                } else if let Some(mode) = directory_mode.filter(|mode| *mode != 0o700) {
                    Check::new(
                        "credential.file",
                        "connection",
                        None,
                        Status::Warn,
                        format!(
                            "credentials directory {} has mode {mode:04o}; expected 0700",
                            path.parent()
                                .map(|parent| parent.display().to_string())
                                .unwrap_or_default()
                        ),
                    )
                    .with_hint(format!(
                        "run `chmod 700 {}`",
                        path.parent()
                            .map(|parent| parent.display().to_string())
                            .unwrap_or_default()
                    ))
                    .with_code("permissions")
                } else {
                    Check::new(
                        "credential.file",
                        "connection",
                        None,
                        Status::Pass,
                        path.display().to_string(),
                    )
                };
                CredentialFile {
                    check,
                    store: Some(store),
                }
            }
        },
    }
}

#[cfg(unix)]
fn permission_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn permission_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn directory_mode(path: &Path) -> Option<u32> {
    fs::metadata(path).ok().as_ref().and_then(permission_mode)
}

#[cfg(not(unix))]
fn directory_mode(path: &Path) -> Option<u32> {
    let _ = path;
    None
}

fn inspect_key(
    store: Option<&CredentialStore>,
    base_url: Option<&BaseUrl>,
    env_key: Option<&str>,
) -> (Check, Option<DoctorKey>) {
    let file_connection = store.and_then(|store| match base_url {
        Some(base_url) => store.connection(base_url.as_str()),
        None => store
            .active
            .as_deref()
            .and_then(|active| store.connection(active)),
    });

    if let Some(env_key) = env_key {
        let mut problems = Vec::new();
        if !credentials::key_has_expected_shape(env_key) {
            problems.push("TOKENCAMP_API_KEY does not look like `sk-` followed by 32 characters");
        }
        if file_connection.is_some() {
            problems.push("TOKENCAMP_API_KEY overrides the stored key");
        }

        let masked = mask_key(env_key);
        let check = if problems.is_empty() {
            Check::new(
                "credential.key",
                "connection",
                None,
                Status::Pass,
                format!("using TOKENCAMP_API_KEY {masked}"),
            )
        } else {
            let hint = if file_connection.is_some() {
                "unset TOKENCAMP_API_KEY to use the stored key; the environment value is never saved"
            } else {
                "re-run `tcamp login --key-stdin` if the key was copied incompletely"
            };
            Check::new(
                "credential.key",
                "connection",
                None,
                Status::Warn,
                format!("using TOKENCAMP_API_KEY {masked}; {}", problems.join("; ")),
            )
            .with_hint(hint)
            .with_code("key_format")
        };

        return (
            check,
            Some(DoctorKey {
                masked,
                source: "env",
                verified: None,
                verified_at: None,
            }),
        );
    }

    if let Some(connection) = file_connection {
        let masked = mask_key(&connection.api_key);
        let check = if credentials::key_has_expected_shape(&connection.api_key) {
            Check::new(
                "credential.key",
                "connection",
                None,
                Status::Pass,
                format!("using stored key {masked}"),
            )
        } else {
            Check::new(
                "credential.key",
                "connection",
                None,
                Status::Warn,
                format!(
                    "using stored key {masked}; the key does not look like `sk-` followed by 32 characters"
                ),
            )
            .with_hint("run `tcamp login --key-stdin` again if the key was copied incompletely")
            .with_code("key_format")
        };
        return (
            check,
            Some(DoctorKey {
                masked,
                source: "file",
                verified: None,
                verified_at: connection.verified_at.clone(),
            }),
        );
    }

    (
        Check::new(
            "credential.key",
            "connection",
            None,
            Status::Fail,
            "no API key was found",
        )
        .with_hint("run `tcamp login --key-stdin` or set TOKENCAMP_API_KEY")
        .with_code("missing_key"),
        None,
    )
}

fn mask_key(key: &str) -> String {
    if key.chars().any(char::is_control) {
        "…".to_string()
    } else {
        credentials::mask_key(key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigState {
    Configured,
    Conflict,
    DriftOnly,
    Unconfigured,
    Missing,
}

struct TargetProbe {
    target: Option<PathBuf>,
    resolved: Option<PathBuf>,
    contents: Option<Vec<u8>>,
    path_error: Option<TcampError>,
    read_error: Option<TcampError>,
    entry: Option<ManifestEntry>,
}

fn inspect_harness(
    harness: Harness,
    manifest: &Result<Manifest, TcampError>,
) -> (HarnessView, Vec<Check>) {
    let probe = probe_target(harness, manifest);
    let (binary_check, installed) = inspect_binary(harness);
    let (config_check, configured) = inspect_config(harness, &probe);
    let ownership_check = inspect_ownership(harness, &probe, manifest);
    let shadowing_check = inspect_shadowing(harness, &probe);
    let models_check = inspect_models(harness);
    let backups_check = inspect_backups(harness, &probe, manifest);

    let mut checks = vec![
        binary_check,
        config_check,
        ownership_check,
        shadowing_check,
        models_check,
        backups_check,
    ];
    if harness == Harness::Claude {
        checks.push(inspect_messages(configured));
    }

    let target = (configured || probe.entry.is_some())
        .then(|| {
            probe
                .resolved
                .as_ref()
                .or(probe.target.as_ref())
                .map(|path| path.display().to_string())
        })
        .flatten();
    (
        HarnessView {
            installed,
            configured,
            target,
        },
        checks,
    )
}

fn probe_target(harness: Harness, manifest: &Result<Manifest, TcampError>) -> TargetProbe {
    let target = match harness::target_path(harness) {
        Ok(target) => target,
        Err(error) => {
            return TargetProbe {
                target: None,
                resolved: None,
                contents: None,
                path_error: Some(error),
                read_error: None,
                entry: None,
            };
        }
    };

    match config::read_target(&target) {
        Ok(existing) => {
            let resolved = existing.resolved.path.clone();
            let entry = manifest_entry(manifest, &resolved);
            TargetProbe {
                target: Some(target),
                resolved: Some(resolved),
                contents: existing.contents,
                path_error: None,
                read_error: None,
                entry,
            }
        }
        Err(error) => {
            let resolved = config::resolve_target(&target)
                .ok()
                .map(|target| target.path);
            let entry = resolved
                .as_ref()
                .and_then(|resolved| manifest_entry(manifest, resolved));
            TargetProbe {
                target: Some(target),
                resolved,
                contents: None,
                path_error: None,
                read_error: Some(error),
                entry,
            }
        }
    }
}

fn manifest_entry(
    manifest: &Result<Manifest, TcampError>,
    resolved: &Path,
) -> Option<ManifestEntry> {
    manifest
        .as_ref()
        .ok()
        .and_then(|manifest| manifest.targets.get(&resolved.display().to_string()))
        .cloned()
}

fn target_label(probe: &TargetProbe) -> String {
    probe
        .resolved
        .as_ref()
        .or(probe.target.as_ref())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn inspect_binary(harness: Harness) -> (Check, Option<String>) {
    let id = format!("harness.{}.binary", harness.id());
    let Some(binary) = harness::launch::binary_path(harness) else {
        return (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Warn,
                format!("{} is not installed", harness.binary()),
            )
            .with_hint(format!(
                "install {} and re-run `tcamp doctor`; tcamp can still write configuration with `tcamp {} --no-launch`",
                harness.binary(),
                harness.id()
            ))
            .with_code("harness_not_found"),
            None,
        );
    };

    let output = match Command::new(&binary).arg("--version").output() {
        Ok(output) => output,
        Err(error) => {
            return (
                Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Fail,
                    format!("could not run `{} --version`: {error}", binary.display()),
                )
                .with_hint("check that the harness binary is executable")
                .with_code("harness_version_failed"),
                None,
            );
        }
    };

    if !output.status.success() {
        return (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                format!(
                    "`{} --version` exited with status {}",
                    binary.display(),
                    output.status.code().unwrap_or(1)
                ),
            )
            .with_hint("reinstall the harness and re-run `tcamp doctor`")
            .with_code("harness_version_failed"),
            None,
        );
    }

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let Some(version) = harness::launch::parse_version(&text) else {
        return (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                format!(
                    "could not parse a version from `{} --version`",
                    binary.display()
                ),
            )
            .with_hint("reinstall the harness and re-run `tcamp doctor`")
            .with_code("harness_version_failed"),
            None,
        );
    };

    let rendered = harness::launch::render_version(version);
    let floor = harness.min_version();
    let floor_version =
        harness::launch::parse_version(floor).expect("the harness version floors are valid semver");
    let check = if version < floor_version {
        Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Warn,
            format!("{rendered} is below the tested version {floor}"),
        )
        .with_hint(format!(
            "upgrade {} to {floor} or newer; the launch still continues",
            harness.binary()
        ))
        .with_code("harness_version")
    } else {
        Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Pass,
            rendered.clone(),
        )
    };

    (check, Some(rendered))
}

fn config_state(harness: Harness, probe: &TargetProbe) -> Result<ConfigState, TcampError> {
    let Some(contents) = probe.contents.as_deref() else {
        return Ok(if probe.entry.is_some() {
            ConfigState::Missing
        } else {
            ConfigState::Unconfigured
        });
    };
    let path = probe.target.as_deref().unwrap_or(Path::new("config"));
    let state = match harness {
        Harness::Claude => {
            let snapshot = claude::inspect_config(contents, path)?;
            if snapshot.configured {
                ConfigState::Configured
            } else {
                ConfigState::Conflict
            }
        }
        Harness::Pi => {
            let snapshot = pi::inspect_status(Some(contents), path, probe.entry.as_ref())?;
            if snapshot.configured {
                ConfigState::Configured
            } else if snapshot.conflict {
                ConfigState::Conflict
            } else if probe.entry.is_some() {
                ConfigState::DriftOnly
            } else {
                ConfigState::Unconfigured
            }
        }
        Harness::Kimi => {
            let snapshot = kimi::inspect_status(Some(contents), path, probe.entry.as_ref())?;
            if snapshot.configured {
                ConfigState::Configured
            } else if snapshot.conflict {
                ConfigState::Conflict
            } else if probe.entry.is_some() {
                ConfigState::DriftOnly
            } else {
                ConfigState::Unconfigured
            }
        }
    };
    Ok(state)
}

fn inspect_config(harness: Harness, probe: &TargetProbe) -> (Check, bool) {
    let id = format!("harness.{}.config", harness.id());
    if let Some(error) = &probe.path_error {
        return (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                format!("cannot determine the config target: {error}"),
            )
            .with_hint(error.hint())
            .with_code(error.code()),
            probe.entry.is_some(),
        );
    }
    if let Some(error) = &probe.read_error {
        return (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                error.to_string(),
            )
            .with_hint(error.hint())
            .with_code(error.code()),
            probe.entry.is_some(),
        );
    }

    let state = match config_state(harness, probe) {
        Ok(state) => state,
        Err(error) => {
            return (
                Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Fail,
                    error.to_string(),
                )
                .with_hint(error.hint())
                .with_code(error.code()),
                probe.entry.is_some(),
            );
        }
    };

    match state {
        ConfigState::Unconfigured => (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Skip,
                "not configured",
            ),
            false,
        ),
        ConfigState::Missing => (
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                format!(
                    "the ownership manifest records {} but the file is missing",
                    target_label(probe)
                ),
            )
            .with_hint(format!(
                "re-run `tcamp {} --no-launch` to recreate it, or run `tcamp uninstall`",
                harness.id()
            ))
            .with_code("config_unreadable"),
            true,
        ),
        ConfigState::Configured | ConfigState::Conflict | ConfigState::DriftOnly => {
            let path = probe
                .resolved
                .as_deref()
                .or(probe.target.as_deref())
                .unwrap_or(Path::new("config"));
            match is_writable(path) {
                Ok(true) => (
                    Check::new(
                        id,
                        "harness",
                        Some(harness.id()),
                        Status::Pass,
                        path.display().to_string(),
                    ),
                    state == ConfigState::Configured,
                ),
                Ok(false) => (
                    Check::new(
                        id,
                        "harness",
                        Some(harness.id()),
                        Status::Fail,
                        format!("{} is not writable", path.display()),
                    )
                    .with_hint("make the file or its nearest parent directory writable")
                    .with_code("permissions"),
                    state == ConfigState::Configured,
                ),
                Err(detail) => (
                    Check::new(
                        id,
                        "harness",
                        Some(harness.id()),
                        Status::Fail,
                        format!("cannot check writability of {}: {detail}", path.display()),
                    )
                    .with_hint("check the file permissions and try again")
                    .with_code("permissions"),
                    state == ConfigState::Configured,
                ),
            }
        }
    }
}

fn is_writable(path: &Path) -> Result<bool, String> {
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.permissions().readonly() {
            return Ok(false);
        }
    }

    let mut current = path.parent();
    while let Some(directory) = current {
        match fs::metadata(directory) {
            Ok(metadata) => return Ok(!metadata.permissions().readonly()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                current = directory.parent();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(false)
}

fn inspect_ownership(
    harness: Harness,
    probe: &TargetProbe,
    manifest: &Result<Manifest, TcampError>,
) -> Check {
    let id = format!("harness.{}.ownership", harness.id());

    if let Err(error) = manifest {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            error.to_string(),
        )
        .with_hint(error.hint())
        .with_code(error.code());
    }
    if let Some(error) = &probe.path_error {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            format!("cannot determine the config target: {error}"),
        )
        .with_hint(error.hint())
        .with_code(error.code());
    }
    if let Some(error) = &probe.read_error {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            error.to_string(),
        )
        .with_hint(error.hint())
        .with_code(error.code());
    }

    let state = match config_state(harness, probe) {
        Ok(state) => state,
        Err(error) => {
            return Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                error.to_string(),
            )
            .with_hint(error.hint())
            .with_code(error.code());
        }
    };

    match state {
        ConfigState::Unconfigured => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Skip,
            "not configured",
        ),
        ConfigState::Missing => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            format!(
                "the ownership manifest records {} but the file is missing",
                target_label(probe)
            ),
        )
        .with_hint(format!(
            "re-run `tcamp {} --no-launch` to recreate it, or run `tcamp uninstall`",
            harness.id()
        ))
        .with_code("config_unreadable"),
        ConfigState::Conflict => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            conflict_message(harness, probe),
        )
        .with_hint(format!(
            "re-run `tcamp {} --force` to back up and adopt the entry, or remove it manually; tcamp never overwrites unowned entries without confirmation",
            harness.id()
        ))
        .with_code("config_conflict"),
        ConfigState::DriftOnly => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Warn,
            format!(
                "the ownership manifest records managed entries, but they are missing from {}",
                target_label(probe)
            ),
        )
        .with_hint(format!(
            "re-run `tcamp {} --no-launch` to recreate them, or run `tcamp uninstall` to clear the manifest",
            harness.id()
        ))
        .with_code("config_drift"),
        ConfigState::Configured => {
            let Some(entry) = probe.entry.as_ref() else {
                return Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Warn,
                    "the config target carries the tcamp marker, but the ownership manifest has no entry",
                )
                .with_hint(format!(
                    "re-run `tcamp {} --no-launch` to record ownership",
                    harness.id()
                ))
                .with_code("config_drift");
            };
            if entry.harness != harness.id() {
                return Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Fail,
                    format!(
                        "the ownership manifest records {} for {} instead of {}",
                        entry.harness,
                        target_label(probe),
                        harness.id()
                    ),
                )
                .with_hint("fix or remove the ownership manifest and re-run configure")
                .with_code("config_unreadable");
            }
            let drifted = probe.contents.as_deref().is_some_and(|contents| {
                entry
                    .last_written_hash
                    .as_deref()
                    .is_some_and(|hash| hash != config::manifest::hash_bytes(contents))
            });
            let backup_problems = backup_problems(entry);
            if drifted || !backup_problems.is_empty() {
                let mut reasons = Vec::new();
                if drifted {
                    reasons.push(format!(
                        "{} was modified outside tcamp; managed values would be rewritten",
                        target_label(probe)
                    ));
                }
                if !backup_problems.is_empty() {
                    reasons.push(format!(
                        "recorded backups are missing or unreadable: {}",
                        backup_problems.join("; ")
                    ));
                }
                let hint = if drifted {
                    "review the file; the next configure overwrites managed fields"
                } else {
                    "backups are kept in the tcamp state directory; re-run configure to create new backups"
                };
                Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Warn,
                    reasons.join("; "),
                )
                .with_hint(hint)
                .with_code(if drifted {
                    "config_drift"
                } else {
                    "backup_missing"
                })
            } else {
                Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Pass,
                    "manifest ok",
                )
            }
        }
    }
}

fn conflict_message(harness: Harness, probe: &TargetProbe) -> String {
    match harness {
        Harness::Claude if probe.entry.is_some() => format!(
            "the Claude overlay {} no longer carries the tcamp marker",
            target_label(probe)
        ),
        Harness::Claude => format!(
            "the Claude overlay {} exists but is not managed by tcamp",
            target_label(probe)
        ),
        Harness::Pi => format!(
            "the Pi provider entry `tokencamp` in {} is not recorded as managed by tcamp",
            target_label(probe)
        ),
        Harness::Kimi => format!(
            "the Kimi Code `tokencamp` entries in {} are not recorded as managed by tcamp",
            target_label(probe)
        ),
    }
}

fn inspect_shadowing(harness: Harness, probe: &TargetProbe) -> Check {
    let id = format!("harness.{}.shadowing", harness.id());
    match harness {
        Harness::Claude => {
            let report = claude::inspect_shadowing();
            if !report.conflicts.is_empty() {
                return Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Fail,
                    report
                        .conflicts
                        .iter()
                        .map(claude::ShadowingConflict::message)
                        .collect::<Vec<_>>()
                        .join("; "),
                )
                .with_hint("remove the token or apiKeyHelper from the Claude settings; `--force` does not bypass shadowing")
                .with_code("shadowing_conflict");
            }
            if !report.unreadable.is_empty() {
                return Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Fail,
                    report
                        .unreadable
                        .iter()
                        .map(claude::ShadowingUnreadable::message)
                        .collect::<Vec<_>>()
                        .join("; "),
                )
                .with_hint(
                    "make the Claude settings readable or remove them and re-run `tcamp doctor`",
                )
                .with_code("shadowing_unreadable");
            }
            if !report.warnings.is_empty() {
                return Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Warn,
                    report
                        .warnings
                        .iter()
                        .map(|warning| warning.message.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
                .with_hint("make the Claude managed settings readable so tcamp can rule out credential shadowing")
                .with_code("shadowing_managed_unreadable");
            }
            Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Pass,
                "no shadowing settings found",
            )
        }
        Harness::Pi => match pi::inspect_shadowing() {
            Err(error) => Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Fail,
                error.to_string(),
            )
            .with_hint(error.hint())
            .with_code(error.code()),
            Ok(report) if !report.is_empty() => {
                let error = report.error();
                Check::new(
                    id,
                    "harness",
                    Some(harness.id()),
                    Status::Fail,
                    error.to_string(),
                )
                .with_hint(error.hint())
                .with_code(error.code())
            }
            Ok(_) => Check::new(
                id,
                "harness",
                Some(harness.id()),
                Status::Pass,
                "auth.json has no tokencamp entry",
            ),
        },
        Harness::Kimi => inspect_kimi_shadowing(harness, probe, id),
    }
}

fn inspect_kimi_shadowing(harness: Harness, probe: &TargetProbe, id: String) -> Check {
    if let Some(error) = &probe.path_error {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            format!("cannot determine the config target: {error}"),
        )
        .with_hint(error.hint())
        .with_code(error.code());
    }
    if let Some(error) = &probe.read_error {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            error.to_string(),
        )
        .with_hint(error.hint())
        .with_code(error.code());
    }

    let Some(contents) = probe.contents.as_deref() else {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Pass,
            "no tokencamp provider entry",
        );
    };
    let path = probe
        .target
        .as_deref()
        .or(probe.resolved.as_deref())
        .unwrap_or(Path::new("config.toml"));
    match kimi::inspect_literal_key(Some(contents), path, probe.entry.as_ref()) {
        Err(error) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            error.to_string(),
        )
        .with_hint(error.hint())
        .with_code(error.code()),
        Ok(kimi::LiteralKey::Absent) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Pass,
            "no literal api_key",
        ),
        Ok(kimi::LiteralKey::Owned) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Warn,
            "the tokencamp provider entry has a literal api_key that the next configure removes",
        )
        .with_hint("re-run `tcamp kimi --no-launch` to replace it with `api_key_env`")
        .with_code("config_drift"),
        Ok(kimi::LiteralKey::Unowned) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            "an unowned tokencamp provider entry has a literal api_key that would shadow TOKENCAMP_API_KEY",
        )
        .with_hint("remove the literal api_key or re-run `tcamp kimi --force` to adopt the entry")
        .with_code("config_conflict"),
        Ok(kimi::LiteralKey::Invalid) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            "`providers.tokencamp.api_key` is not a string",
        )
        .with_hint("remove the value or re-run `tcamp kimi --force` to adopt the entry")
        .with_code("config_unreadable"),
    }
}

fn inspect_models(harness: Harness) -> Check {
    let id = format!("harness.{}.models", harness.id());
    match static_mapping(harness) {
        Ok(ids) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Pass,
            format!("{} mapped model(s): {}", ids.len(), ids.join(", ")),
        ),
        Err(problems) => Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Fail,
            format!(
                "the static model mapping is invalid: {}",
                problems.join("; ")
            ),
        )
        .with_hint("upgrade tcamp to a release with a complete curated mapping")
        .with_code("model_mapping"),
    }
}

fn static_mapping(harness: Harness) -> Result<Vec<String>, Vec<String>> {
    let (ids, aliases) = match harness {
        Harness::Claude => {
            let mut expressions = vec![models::DEFAULT_MAIN_MODEL.to_string()];
            expressions.extend(
                models::CLAUDE_SLOTS
                    .iter()
                    .map(|model| (*model).to_string()),
            );
            if expressions.len() != 5 {
                return Err(vec![format!(
                    "expected a main model and four Claude slots, found {} entries",
                    expressions.len()
                )]);
            }
            (expressions, None)
        }
        Harness::Pi => (
            pi::exposed_ids().into_iter().map(str::to_string).collect(),
            None,
        ),
        Harness::Kimi => (
            kimi::exposed_model_ids()
                .into_iter()
                .map(str::to_string)
                .collect(),
            Some(
                kimi::exposed_aliases()
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            ),
        ),
    };
    let mut ids = ids;
    apply_test_mapping_break(harness, &mut ids);

    let mut problems = Vec::new();
    if ids.len() != 5 {
        problems.push(format!("expected 5 mapped models, found {}", ids.len()));
    }
    let mut unique: Vec<String> = Vec::new();
    for id in &ids {
        if !unique.contains(id) {
            unique.push(id.clone());
        }
        if id.is_empty()
            || id == "auto"
            || id.starts_with("@preset/")
            || id.starts_with('~')
            || models::catalog_model(id).is_none()
        {
            problems.push(format!("{id:?} is not a usable Catalog model"));
        }
    }
    if let Some(aliases) = aliases {
        if aliases.len() != ids.len() {
            problems.push(format!(
                "expected one alias per model, found {} aliases",
                aliases.len()
            ));
        }
        let mut unique_aliases: Vec<String> = Vec::new();
        for alias in &aliases {
            if unique_aliases.contains(alias) {
                problems.push(format!("duplicate alias {alias:?}"));
            }
            unique_aliases.push(alias.clone());
        }
    }

    if problems.is_empty() {
        Ok(unique)
    } else {
        Err(problems)
    }
}

#[cfg(debug_assertions)]
fn apply_test_mapping_break(harness: Harness, ids: &mut Vec<String>) {
    if std::env::var("TCAMP_TEST_DOCTOR_MODEL_GAP").ok().as_deref() == Some(harness.id()) {
        ids.pop();
    }
    if std::env::var("TCAMP_TEST_DOCTOR_MODEL_DISABLED")
        .ok()
        .as_deref()
        == Some(harness.id())
        && !ids.is_empty()
    {
        ids[0] = "auto".to_string();
    }
}

#[cfg(not(debug_assertions))]
fn apply_test_mapping_break(_harness: Harness, _ids: &mut Vec<String>) {}

fn backup_problems(entry: &ManifestEntry) -> Vec<String> {
    let mut problems = Vec::new();
    for path in &entry.backups {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                problems.push(format!("{path} is a symlink"));
            }
            Ok(metadata) if !metadata.is_file() => {
                problems.push(format!("{path} is not a regular file"));
            }
            Ok(_) => {
                if fs::File::open(path).is_err() {
                    problems.push(format!("{path} is unreadable"));
                }
            }
            Err(_) => problems.push(format!("{path} is missing")),
        }
    }
    problems
}

fn inspect_backups(
    harness: Harness,
    probe: &TargetProbe,
    manifest: &Result<Manifest, TcampError>,
) -> Check {
    let id = format!("harness.{}.backups", harness.id());

    if manifest.is_err() {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Skip,
            "ownership manifest unavailable",
        );
    }
    let Some(entry) = probe.entry.as_ref() else {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Skip,
            "no backups recorded",
        );
    };
    if entry.backups.is_empty() {
        return Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Skip,
            "no backups recorded",
        );
    }

    let problems = backup_problems(entry);

    if problems.is_empty() {
        Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Pass,
            format!(
                "{} backup{} present",
                entry.backups.len(),
                if entry.backups.len() == 1 { "" } else { "s" }
            ),
        )
    } else {
        Check::new(
            id,
            "harness",
            Some(harness.id()),
            Status::Warn,
            format!("backup check failed: {}", problems.join("; ")),
        )
        .with_hint(
            "backups are kept in the tcamp state directory; re-run configure to create new backups",
        )
        .with_code("backup_missing")
    }
}

fn inspect_messages(configured: bool) -> Check {
    if !configured {
        return Check::new(
            "harness.claude.messages",
            "harness",
            Some("claude"),
            Status::Skip,
            "not configured",
        );
    }
    Check::new(
        "harness.claude.messages",
        "harness",
        Some("claude"),
        Status::Warn,
        "/v1/messages capability not verified",
    )
    .with_hint(
        "requires an anthropic deployment; run `tcamp doctor --verify --harness claude` to test",
    )
    .with_code("messages_unverified")
}

fn summarize(checks: &[Check]) -> Summary {
    let mut summary = Summary::default();
    for check in checks {
        match check.status {
            Status::Pass => summary.pass += 1,
            Status::Warn => summary.warn += 1,
            Status::Fail => summary.fail += 1,
            Status::Skip => summary.skip += 1,
        }
    }
    summary
}

const PASS_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const WARN_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const FAIL_STYLE: Style = Style::new()
    .bold()
    .fg_color(Some(Color::Ansi(AnsiColor::Red)));
const SKIP_STYLE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

fn print_board(
    report: &DoctorReport,
    selected: &[Harness],
    verbose: bool,
    color: ColorChoice,
) -> Result<(), TcampError> {
    let mut writer = AutoStream::new(io::stdout().lock(), color);
    write_board(&mut writer, report, selected, verbose).map_err(|error| {
        TcampError::io(
            format!("failed to write the doctor report: {error}"),
            "check that stdout is writable and try again",
        )
    })
}

fn write_board<W: Write>(
    writer: &mut W,
    report: &DoctorReport,
    selected: &[Harness],
    verbose: bool,
) -> io::Result<()> {
    writeln!(writer, "tcamp doctor")?;
    writeln!(writer, "Connection")?;
    for check in report
        .checks
        .iter()
        .filter(|check| check.scope == "connection")
    {
        print_check(writer, check, "  ", &check.id, verbose)?;
    }
    writeln!(writer, "Network")?;
    for check in report
        .checks
        .iter()
        .filter(|check| check.scope == "network")
    {
        print_check(writer, check, "  ", &check.id, verbose)?;
    }
    writeln!(writer, "Harnesses")?;
    for harness in selected {
        writeln!(writer, "  {}", harness.id())?;
        let prefix = format!("harness.{}.", harness.id());
        for check in report
            .checks
            .iter()
            .filter(|check| check.harness == Some(harness.id()))
        {
            let short = check.id.strip_prefix(&prefix).unwrap_or(&check.id);
            print_check(writer, check, "    ", short, verbose)?;
        }
    }
    let summary = &report.summary;
    writeln!(
        writer,
        "Summary: {} pass, {} warn, {} fail, {} skip",
        summary.pass, summary.warn, summary.fail, summary.skip
    )?;
    writer.flush()
}

fn print_check<W: Write>(
    writer: &mut W,
    check: &Check,
    indent: &str,
    label: &str,
    verbose: bool,
) -> io::Result<()> {
    let style = if check.status == Status::Warn
        && check.code.as_deref() == Some("shadowing_managed_unreadable")
    {
        FAIL_STYLE
    } else {
        match check.status {
            Status::Pass => PASS_STYLE,
            Status::Warn => WARN_STYLE,
            Status::Fail => FAIL_STYLE,
            Status::Skip => SKIP_STYLE,
        }
    };
    write!(
        writer,
        "{indent}{style}{:<4}{style:#}  {:<20} {}",
        check.status.as_str(),
        label,
        check.message
    )?;
    if let Some(code) = &check.code {
        write!(writer, " [{code}]")?;
    }
    writeln!(writer)?;
    if let Some(hint) = &check.hint {
        writeln!(writer, "{indent}  hint: {hint}")?;
    }
    if verbose {
        if let Some(request_id) = &check.request_id {
            writeln!(writer, "{indent}  request_id: {request_id}")?;
        }
    }
    Ok(())
}
