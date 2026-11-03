pub mod backup;
pub mod diff;
pub mod engine;
pub mod fs;
pub mod lock;
pub mod manifest;
pub mod paths;

pub use fs::{ResolvedTarget, TargetContents, read_target, resolve_target};
pub use paths::{claude_overlay_path, manifest_path, state_home, tcamp_config_dir};
