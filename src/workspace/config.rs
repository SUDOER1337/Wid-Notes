use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkspaceConfig {
    pub name: String,
    pub created_at: String,
}

impl WorkspaceConfig {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    pub fn load(path: &Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let content = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, content)
    }
}
