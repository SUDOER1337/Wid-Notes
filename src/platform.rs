//! Platform-specific path helpers shared across modules.

use std::path::PathBuf;

/// Returns the Wid config directory: `~/.config/wid/` on Unix/macOS,
/// or `%USERPROFILE%\.config\wid\` on Windows.
pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("wid")
}
