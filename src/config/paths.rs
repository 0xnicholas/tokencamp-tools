use std::env;
use std::path::PathBuf;

use crate::errors::TcampError;

pub fn config_home() -> Result<PathBuf, TcampError> {
    home_from("XDG_CONFIG_HOME", ".config")
}

pub fn state_home() -> Result<PathBuf, TcampError> {
    home_from("XDG_STATE_HOME", ".local/state")
}

fn home_from(variable: &str, fallback: &str) -> Result<PathBuf, TcampError> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(fallback)))
        .ok_or_else(|| {
            TcampError::config_unreadable(
                format!("cannot determine the base directory (checked {variable} and HOME)"),
                format!("set HOME or {variable} and try again"),
            )
        })
}

pub fn tcamp_config_dir() -> Result<PathBuf, TcampError> {
    Ok(config_home()?.join("tokencamp"))
}

pub fn claude_overlay_path() -> Result<PathBuf, TcampError> {
    Ok(tcamp_config_dir()?
        .join("harnesses")
        .join("claude")
        .join("settings.json"))
}

pub fn manifest_path() -> Result<PathBuf, TcampError> {
    Ok(tcamp_config_dir()?.join("managed.toml"))
}

pub fn backups_dir(harness: &str) -> Result<PathBuf, TcampError> {
    Ok(state_home()?
        .join("tokencamp")
        .join("backups")
        .join(harness))
}

pub fn locks_dir() -> Result<PathBuf, TcampError> {
    Ok(state_home()?.join("tokencamp").join("locks"))
}
