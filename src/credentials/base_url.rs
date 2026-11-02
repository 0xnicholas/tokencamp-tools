use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use crate::errors::TcampError;

pub const DEFAULT_BASE_URL: &str = "https://api.tokencamp.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl {
    normalized: String,
    scheme: Scheme,
    host: String,
    local_host: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    Http,
    Https,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpPolicy {
    Secure,
    InsecureLocal,
    InsecurePublic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrlError {
    message: String,
    hint: String,
}

impl BaseUrlError {
    fn new(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: hint.into(),
        }
    }
}

impl From<BaseUrlError> for TcampError {
    fn from(error: BaseUrlError) -> Self {
        TcampError::usage(error.message, error.hint)
    }
}

impl BaseUrl {
    pub fn parse(input: &str) -> Result<Self, BaseUrlError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(BaseUrlError::new(
                "the base URL is empty",
                "pass a URL such as https://api.tokencamp.com",
            ));
        }
        if trimmed
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(BaseUrlError::new(
                "the base URL contains whitespace or control characters",
                "pass a single URL without spaces",
            ));
        }

        let (scheme, rest) = trimmed.split_once("://").ok_or_else(|| {
            BaseUrlError::new(
                format!("{trimmed:?} is not a valid URL"),
                "include the scheme, for example https://api.tokencamp.com",
            )
        })?;
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "https" => Scheme::Https,
            "http" => Scheme::Http,
            other => {
                return Err(BaseUrlError::new(
                    format!("unsupported URL scheme {other:?}"),
                    "use an https:// base URL; plain http:// is only allowed on loopback or private networks",
                ));
            }
        };

        if rest.is_empty() {
            return Err(BaseUrlError::new(
                "the base URL has no host",
                "include a host, for example https://api.tokencamp.com",
            ));
        }
        if rest.contains('?') || rest.contains('#') {
            return Err(BaseUrlError::new(
                "the base URL must not contain a query or fragment",
                "pass only the base URL, without query parameters or fragments",
            ));
        }
        if rest.contains('@') {
            return Err(BaseUrlError::new(
                "the base URL must not contain user information",
                "remove the user:password prefix and pass only the base URL",
            ));
        }

        let path_start = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..path_start];
        let path = &rest[path_start..];
        if authority.is_empty() {
            return Err(BaseUrlError::new(
                "the base URL has no host",
                "include a host, for example https://api.tokencamp.com",
            ));
        }

        let (host, _port) = split_authority(authority)?;
        let mut normalized_path = path.trim_end_matches('/');
        if normalized_path.ends_with("/v1") {
            normalized_path = &normalized_path[..normalized_path.len() - 3];
            normalized_path = normalized_path.trim_end_matches('/');
        }
        let normalized = format!(
            "{}://{}{}",
            scheme.as_str(),
            authority.to_ascii_lowercase(),
            normalized_path
        );

        Ok(Self {
            normalized,
            scheme,
            local_host: host_is_local(&host),
            host,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    pub fn join(&self, path: &str) -> String {
        debug_assert!(path.starts_with('/'));
        format!("{}{}", self.normalized, path)
    }

    pub fn http_policy(&self) -> HttpPolicy {
        match (self.scheme, self.local_host) {
            (Scheme::Https, _) => HttpPolicy::Secure,
            (Scheme::Http, true) => HttpPolicy::InsecureLocal,
            (Scheme::Http, false) => HttpPolicy::InsecurePublic,
        }
    }

    pub fn is_official_cloud(&self) -> bool {
        self.host == "tokencamp.com"
            || self.host == "api.tokencamp.com"
            || self.host.ends_with(".tokencamp.com")
    }

    pub fn portal_keys_url(&self) -> Option<&'static str> {
        self.is_official_cloud()
            .then_some("https://tokencamp.com/keys")
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.normalized)
    }
}

impl Scheme {
    fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

fn split_authority(authority: &str) -> Result<(String, Option<u16>), BaseUrlError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']').ok_or_else(|| {
            BaseUrlError::new(
                "the base URL has an unterminated IPv6 address",
                "close the IPv6 address with `]`",
            )
        })?;
        let host = &rest[..end];
        if host.parse::<Ipv6Addr>().is_err() {
            return Err(BaseUrlError::new(
                format!("{host:?} is not a valid IPv6 address"),
                "fix the IPv6 address in the base URL",
            ));
        }
        let after = &rest[end + 1..];
        let port = if after.is_empty() {
            None
        } else if let Some(port) = after.strip_prefix(':') {
            Some(parse_port(port)?)
        } else {
            return Err(BaseUrlError::new(
                "the base URL has trailing text after the IPv6 address",
                "pass only the base URL",
            ));
        };
        return Ok((host.to_ascii_lowercase(), port));
    }

    if authority.matches(':').count() > 1 {
        return Err(BaseUrlError::new(
            "IPv6 addresses in the base URL must be enclosed in brackets",
            "write the address as http://[::1]:8080",
        ));
    }

    if let Some((host, port)) = authority.rsplit_once(':') {
        if port.is_empty() {
            return Err(BaseUrlError::new(
                "the base URL has an empty port",
                "remove the trailing colon or provide a port",
            ));
        }
        if !port.chars().all(|character| character.is_ascii_digit()) {
            return Err(BaseUrlError::new(
                format!("{port:?} is not a valid port"),
                "use a numeric port between 1 and 65535",
            ));
        }
        let port = parse_port(port)?;
        validate_host(host)?;
        return Ok((host.to_ascii_lowercase(), Some(port)));
    }

    validate_host(authority)?;
    Ok((authority.to_ascii_lowercase(), None))
}

fn parse_port(port: &str) -> Result<u16, BaseUrlError> {
    port.parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            BaseUrlError::new(
                format!("{port:?} is not a valid port"),
                "use a numeric port between 1 and 65535",
            )
        })
}

fn validate_host(host: &str) -> Result<(), BaseUrlError> {
    if host.is_empty() {
        return Err(BaseUrlError::new(
            "the base URL has no host",
            "include a host, for example https://api.tokencamp.com",
        ));
    }
    if !host
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_'))
    {
        return Err(BaseUrlError::new(
            format!("{host:?} is not a valid host"),
            "use a hostname, IPv4 address, or bracketed IPv6 address",
        ));
    }
    Ok(())
}

fn host_is_local(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    if let Ok(ipv4) = Ipv4Addr::from_str(host) {
        return ipv4.is_loopback() || ipv4.is_private() || ipv4.is_link_local();
    }
    if let Ok(ipv6) = Ipv6Addr::from_str(host) {
        if let Some(ipv4) = ipv6.to_ipv4_mapped() {
            return ipv4.is_loopback() || ipv4.is_private() || ipv4.is_link_local();
        }
        return ipv6.is_loopback() || ipv6.is_unique_local() || ipv6.is_unicast_link_local();
    }
    false
}
