//! Platform-specific path helpers shared across modules.

use std::path::PathBuf;

/// Returns the Nortex config directory: `~/.config/nortex/` on Unix/macOS,
/// or `%USERPROFILE%\.config\nortex\` on Windows.
pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("nortex")
}
