mod base_url;
mod probe;
mod store;

use std::env;
use std::io::{self, Read};
use std::path::PathBuf;

use crate::cli::{LoginArgs, LogoutArgs};
use crate::errors::TcampError;
use crate::terminal;
pub use crate::warnings::Warning;

use base_url::{BaseUrl, DEFAULT_BASE_URL, HttpPolicy};
use probe::{ProbeResult, VerifyResult};
use store::{Connection, CredentialStore, now};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Stdin,
    Tty,
    Env,
    File,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdin => "stdin",
            Self::Tty => "tty",
            Self::Env => "env",
            Self::File => "file",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoginReport {
    pub base_url: String,
    pub masked_key: String,
    pub source: KeySource,
    pub verified_at: Option<String>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone)]
pub struct LogoutReport {
    pub all: bool,
    pub base_url: Option<String>,
    pub removed: Vec<String>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone)]
pub struct ConnectionContext {
    pub base_url: String,
    pub masked_key: Option<String>,
    pub source: Option<KeySource>,
    pub verified_at: Option<String>,
    pub credentials_path: PathBuf,
    pub warnings: Vec<Warning>,
}

impl ConnectionContext {
    pub fn require_key(self) -> Result<Self, TcampError> {
        if self.masked_key.is_some() {
            Ok(self)
        } else {
            Err(missing_key_error())
        }
    }
}

enum VerificationFailure {
    Rejected {
        code: Option<String>,
        request_id: Option<String>,
    },
    Indeterminate {
        detail: String,
        request_id: Option<String>,
    },
}

pub fn ensure_launch_key(
    mut connection: ConnectionContext,
    interactive: bool,
) -> Result<ConnectionContext, TcampError> {
    if connection.masked_key.is_some() {
        return Ok(connection);
    }
    if !interactive {
        return Err(missing_key_error());
    }

    let api_key = prompt_hidden_key()?;
    let base_url = BaseUrl::parse(&connection.base_url)?;
    let mut warnings = connection.warnings.clone();

    let verified_at = match run_verification(&base_url, &api_key, false) {
        Ok((timestamp, probe_warnings)) => {
            warnings.extend(probe_warnings);
            Some(timestamp)
        }
        Err(VerificationFailure::Rejected { code, request_id }) => {
            return Err(auth_error(&base_url, code, request_id));
        }
        Err(VerificationFailure::Indeterminate { detail, request_id }) => {
            return Err(indeterminate_error(detail, request_id));
        }
    };

    if !key_has_expected_shape(&api_key) {
        warnings.push(key_format_warning());
    }

    let mut store = CredentialStore::load()?;
    store.connections.insert(
        connection.base_url.clone(),
        Connection {
            api_key: api_key.clone(),
            created_at: now(),
            verified_at: verified_at.clone(),
        },
    );
    store.active = Some(connection.base_url.clone());
    store.save()?;

    connection.masked_key = Some(mask_key(&api_key));
    connection.source = Some(KeySource::Tty);
    connection.verified_at = verified_at;
    connection.warnings = warnings;
    Ok(connection)
}

pub fn login(args: LoginArgs, json: bool) -> Result<Option<LoginReport>, TcampError> {
    let mut store = CredentialStore::load()?;
    let base_url = resolve_base_url(args.base_url.as_deref(), &store)?;
    let mut warnings = Vec::new();
    check_transport(&base_url, &mut warnings)?;

    let interactive = !json && terminal::is_tty();
    let (mut api_key, source) = acquire_key(&args, interactive)?;

    let existed = store.connection(base_url.as_str()).is_some();
    if existed && !args.force {
        if !interactive {
            return Err(TcampError::config_conflict(
                format!("an API key is already stored for {base_url}"),
                "re-run with `--force` to replace the existing key",
            ));
        }
        let confirmation = terminal::confirm(&format!(
            "An API key is already stored for {base_url}. Replace it? [y/N] "
        ))
        .map_err(|error| {
            TcampError::io(
                format!("failed to read the confirmation: {error}"),
                "re-run with `--force` to replace the existing key without a prompt",
            )
        })?;
        match confirmation {
            Some(true) => {}
            Some(false) => return Ok(None),
            None => {
                return Err(TcampError::config_conflict(
                    format!("cannot confirm replacing the API key for {base_url}"),
                    "re-run with `--force` to replace the existing key",
                ));
            }
        }
    }

    let mut verified_at = None;
    if !args.no_verify {
        let mut retried = false;
        loop {
            match run_verification(&base_url, &api_key, args.verify) {
                Ok((timestamp, attempt_warnings)) => {
                    warnings.extend(attempt_warnings);
                    verified_at = Some(timestamp);
                    break;
                }
                Err(VerificationFailure::Rejected { .. })
                    if interactive && source == KeySource::Tty && !retried =>
                {
                    retried = true;
                    eprintln!("warning: the API key was rejected; try again.");
                    api_key = prompt_hidden_key()?;
                }
                Err(VerificationFailure::Rejected { code, request_id }) => {
                    return Err(auth_error(&base_url, code, request_id));
                }
                Err(VerificationFailure::Indeterminate { detail, request_id }) => {
                    return Err(indeterminate_error(detail, request_id));
                }
            }
        }
    }

    if !key_has_expected_shape(&api_key) {
        warnings.push(key_format_warning());
    }
    if existed {
        warnings.push(Warning {
            code: "credential_replaced".to_string(),
            message: "the API key was replaced; restart running harnesses to use the new key"
                .to_string(),
        });
    }

    store.connections.insert(
        base_url.as_str().to_string(),
        Connection {
            api_key: api_key.clone(),
            created_at: now(),
            verified_at: verified_at.clone(),
        },
    );
    store.active = Some(base_url.as_str().to_string());
    store.save()?;

    Ok(Some(LoginReport {
        base_url: base_url.as_str().to_string(),
        masked_key: mask_key(&api_key),
        source,
        verified_at,
        warnings,
    }))
}

pub fn logout(args: LogoutArgs) -> Result<LogoutReport, TcampError> {
    let mut store = CredentialStore::load()?;
    let all = args.all;
    let mut removed = Vec::new();
    let mut target = None;
    let mut changed = false;

    if all {
        changed = !store.connections.is_empty() || store.active.is_some();
        removed = store.connections.keys().cloned().collect();
        store.connections.clear();
        store.active = None;
    } else {
        target = match args.base_url.as_deref() {
            Some(raw) => Some(BaseUrl::parse(raw)?.as_str().to_string()),
            None => store.active.clone(),
        };
        if let Some(base_url) = &target {
            if store.connections.remove(base_url).is_some() {
                removed.push(base_url.clone());
                changed = true;
            }
            if store.active.as_deref() == Some(base_url) {
                store.active = None;
                changed = true;
            }
        }
    }

    if changed {
        store.save()?;
    }

    let mut warnings = Vec::new();
    if !removed.is_empty() {
        warnings.push(Warning {
            code: "harness_restart_required".to_string(),
            message: "running harnesses keep the old API key until restarted; revoke the key in the Portal to invalidate it immediately".to_string(),
        });
    }

    Ok(LogoutReport {
        all,
        base_url: if all { None } else { target },
        removed,
        warnings,
    })
}

pub fn status() -> Result<ConnectionContext, TcampError> {
    resolve_connection(None)
}

pub fn require_launch_key(base_url: &str) -> Result<String, TcampError> {
    launch_key(base_url)?.ok_or_else(missing_key_error)
}

pub fn launch_key(base_url: &str) -> Result<Option<String>, TcampError> {
    if let Some(key) = env_key() {
        return Ok(Some(key));
    }
    let store = CredentialStore::load()?;
    Ok(store
        .connection(base_url)
        .map(|connection| connection.api_key.clone()))
}

pub fn resolve_connection(
    explicit_base_url: Option<&str>,
) -> Result<ConnectionContext, TcampError> {
    let store = CredentialStore::load()?;
    let base_url = resolve_base_url(explicit_base_url, &store)?;
    let mut warnings = Vec::new();
    check_transport(&base_url, &mut warnings)?;

    let (masked_key, source, verified_at) = match env_key() {
        Some(key) => (Some(mask_key(&key)), Some(KeySource::Env), None),
        None => match store.connection(base_url.as_str()) {
            Some(connection) => (
                Some(mask_key(&connection.api_key)),
                Some(KeySource::File),
                connection.verified_at.clone(),
            ),
            None => (None, None, None),
        },
    };

    Ok(ConnectionContext {
        base_url: base_url.as_str().to_string(),
        masked_key,
        source,
        verified_at,
        credentials_path: store.path,
        warnings,
    })
}

fn acquire_key(args: &LoginArgs, interactive: bool) -> Result<(String, KeySource), TcampError> {
    if args.key_stdin {
        return read_stdin_key().map(|key| (key, KeySource::Stdin));
    }
    if interactive {
        return prompt_hidden_key().map(|key| (key, KeySource::Tty));
    }
    if let Some(key) = env_key() {
        return validate_key(&key).map(|key| (key, KeySource::Env));
    }
    Err(missing_key_error())
}

fn read_stdin_key() -> Result<String, TcampError> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|error| {
        TcampError::io(
            format!("failed to read the API key from stdin: {error}"),
            "pipe the key into `tcamp login --key-stdin`",
        )
    })?;
    validate_key(&input)
}

fn prompt_hidden_key() -> Result<String, TcampError> {
    match terminal::prompt_hidden("API key: ") {
        Ok(Some(key)) => validate_key(&key),
        Ok(None) => Err(missing_key_error()),
        Err(error) => Err(TcampError::io(
            format!("failed to read the API key: {error}"),
            "run `tcamp login --key-stdin` and pipe the key through stdin",
        )),
    }
}

fn validate_key(raw: &str) -> Result<String, TcampError> {
    let key = raw.trim();
    if key.is_empty() {
        return Err(missing_key_error());
    }
    if key.chars().any(char::is_control) {
        return Err(TcampError::usage(
            "the API key contains control characters",
            "provide the key as a single line without control characters",
        ));
    }
    Ok(key.to_string())
}

fn env_key() -> Option<String> {
    env::var("TOKENCAMP_API_KEY")
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

fn missing_key_error() -> TcampError {
    TcampError::missing_key(
        "no API key was provided",
        "run `tcamp login --key-stdin` or set TOKENCAMP_API_KEY",
    )
}

fn key_has_expected_shape(key: &str) -> bool {
    key.starts_with("sk-") && key.chars().count() == 35
}

fn key_format_warning() -> Warning {
    Warning {
        code: "key_format".to_string(),
        message:
            "the API key does not look like `sk-` followed by 32 characters; it was saved anyway"
                .to_string(),
    }
}

pub fn mask_key(key: &str) -> String {
    let length = key.chars().count();
    if length < 8 {
        return "…".to_string();
    }
    let suffix: String = key.chars().skip(length - 4).collect();
    format!("sk-…{suffix}")
}

fn resolve_base_url(
    explicit: Option<&str>,
    store: &CredentialStore,
) -> Result<BaseUrl, TcampError> {
    let raw = if let Some(explicit) = explicit {
        explicit.to_string()
    } else if let Some(from_env) = env::var("TOKENCAMP_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        from_env
    } else if let Some(active) = &store.active {
        active.clone()
    } else {
        DEFAULT_BASE_URL.to_string()
    };
    Ok(BaseUrl::parse(&raw)?)
}

fn check_transport(base_url: &BaseUrl, warnings: &mut Vec<Warning>) -> Result<(), TcampError> {
    match base_url.http_policy() {
        HttpPolicy::Secure => Ok(()),
        HttpPolicy::InsecureLocal => {
            warnings.push(Warning {
                code: "insecure_http".to_string(),
                message: format!(
                    "plain HTTP is in use for {base_url}; the API key is not encrypted in transit"
                ),
            });
            Ok(())
        }
        HttpPolicy::InsecurePublic => Err(TcampError::usage(
            format!("refusing to send an API key over plain HTTP to {base_url}"),
            "use https:// or a loopback, RFC1918, or link-local address",
        )),
    }
}

fn run_verification(
    base_url: &BaseUrl,
    api_key: &str,
    verify: bool,
) -> Result<(String, Vec<Warning>), VerificationFailure> {
    let mut warnings = Vec::new();
    match probe::probe(base_url, api_key) {
        ProbeResult::Verified { warning } => {
            if let Some(warning) = warning {
                warnings.push(warning);
            }
        }
        ProbeResult::Rejected { code, request_id } => {
            return Err(VerificationFailure::Rejected { code, request_id });
        }
        ProbeResult::Indeterminate { detail, request_id } => {
            return Err(VerificationFailure::Indeterminate { detail, request_id });
        }
    }

    if verify {
        match probe::verify_chat(base_url, api_key) {
            VerifyResult::Verified => {}
            VerifyResult::Warning(warning) => warnings.push(warning),
            VerifyResult::Rejected { code, request_id } => {
                return Err(VerificationFailure::Rejected { code, request_id });
            }
            VerifyResult::Indeterminate { detail, request_id } => {
                return Err(VerificationFailure::Indeterminate { detail, request_id });
            }
        }
    }

    Ok((now(), warnings))
}

fn auth_error(base_url: &BaseUrl, code: Option<String>, request_id: Option<String>) -> TcampError {
    let hint = match base_url.portal_keys_url() {
        Some(portal) => format!("create or rotate an API key at {portal}"),
        None => format!("check the API key for {base_url}"),
    };
    let error = TcampError::authentication(format!("the API key was rejected by {base_url}"), hint);
    let error = match code {
        Some(code) => error.with_code(code),
        None => error,
    };
    match request_id {
        Some(request_id) => error.with_request_id(request_id),
        None => error,
    }
}

fn indeterminate_error(detail: String, request_id: Option<String>) -> TcampError {
    let error = TcampError::network(
        detail,
        "check connectivity, or re-run with `--no-verify` to save the key without verification",
    );
    match request_id {
        Some(request_id) => error.with_request_id(request_id),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_failure_hint_points_official_cloud_at_the_portal() {
        let official = BaseUrl::parse("https://api.tokencamp.com").expect("official base URL");
        let error = auth_error(&official, None, None);
        assert!(error.hint().contains("https://tokencamp.com/keys"));

        let self_hosted = BaseUrl::parse("https://self-hosted.test").expect("self-hosted base URL");
        let error = auth_error(&self_hosted, None, None);
        assert!(!error.hint().contains("/keys"));
        assert!(error.hint().contains("https://self-hosted.test"));
    }

    #[test]
    fn remote_codes_and_request_ids_are_preserved() {
        let base_url = BaseUrl::parse("https://self-hosted.test").expect("base URL");
        let error = auth_error(
            &base_url,
            Some("authentication".to_string()),
            Some("req_1".to_string()),
        );
        assert_eq!(error.code(), "authentication");
        assert_eq!(error.request_id(), Some("req_1"));
    }

    #[test]
    fn short_keys_are_fully_masked() {
        assert_eq!(mask_key("abc"), "…");
        assert_eq!(mask_key("sk-0123456789abcdef0123456789abcdef"), "sk-…cdef");
    }
}
