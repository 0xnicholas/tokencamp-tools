use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Usage,
    MissingKey,
    HarnessNotFound,
    ConfigConflict,
    ConfigUnreadable,
    Network,
    Authentication,
    Io,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::MissingKey => "missing_key",
            Self::HarnessNotFound => "harness_not_found",
            Self::ConfigConflict => "config_conflict",
            Self::ConfigUnreadable => "config_unreadable",
            Self::Network => "network",
            Self::Authentication => "authentication",
            Self::Io => "io",
        }
    }

    pub fn exit_code(self) -> u8 {
        match self {
            Self::Network | Self::Authentication | Self::Io => 1,
            _ => 2,
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct TcampError {
    code: ErrorCode,
    remote_code: Option<String>,
    message: String,
    hint: String,
    request_id: Option<String>,
}

impl TcampError {
    pub fn new(code: ErrorCode, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            code,
            remote_code: None,
            message: message.into(),
            hint: hint.into(),
            request_id: None,
        }
    }

    pub fn usage(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message, hint)
    }

    pub fn missing_key(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::MissingKey, message, hint)
    }

    pub fn harness_not_found(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::HarnessNotFound, message, hint)
    }

    pub fn config_conflict(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::ConfigConflict, message, hint)
    }

    pub fn config_unreadable(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::ConfigUnreadable, message, hint)
    }

    pub fn network(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::Network, message, hint)
    }

    pub fn authentication(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::Authentication, message, hint)
    }

    pub fn io(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message, hint)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.remote_code = Some(code.into());
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn not_implemented(command: &str) -> Self {
        Self::usage(
            format!("`tcamp {command}` is not implemented yet"),
            "this build only ships the command-line shell; run `tcamp help` to see the command tree",
        )
    }

    pub fn code(&self) -> &str {
        self.remote_code
            .as_deref()
            .unwrap_or_else(|| self.code.as_str())
    }

    pub fn hint(&self) -> &str {
        &self.hint
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn exit_code(&self) -> u8 {
        self.code.exit_code()
    }
}
