// File: src/session.rs
// Functions:
//   save_last_workspace(path)  - Persist last workspace path
//   load_last_workspace()      - Restore last workspace path

use std::fs;
use std::path::{Path, PathBuf};

fn session_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/nortex/last_workspace")
}

/// Persist the last opened workspace path to a file.
pub fn save_last_workspace(path: &Path) {
    if let Some(parent) = session_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(session_path(), path.to_string_lossy().as_bytes());
}

/// Load the previously saved workspace path, if it exists.
pub fn load_last_workspace() -> Option<PathBuf> {
    let path = session_path();
    if path.exists() {
        fs::read_to_string(path).ok().map(PathBuf::from)
    } else {
        None
    }
}
