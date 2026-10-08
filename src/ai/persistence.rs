use crate::ai::types::Conversation;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn default_version() -> u32 {
    1
}

#[derive(Serialize, Deserialize)]
struct SaveData {
    #[serde(default = "default_version")]
    version: u32,
    conversations: Vec<Conversation>,
}

fn data_dir(root: &Path) -> std::path::PathBuf {
    root.join(".wid")
}

fn data_path(root: &Path) -> std::path::PathBuf {
    data_dir(root).join("conversations.json")
}

pub fn save(root: &Path, conversations: &[Conversation]) -> Result<()> {
    let dir = data_dir(root);
    std::fs::create_dir_all(&dir).context("Failed to create .wid directory")?;

    let data = SaveData {
        version: 1,
        conversations: conversations.to_vec(),
    };
    let json = serde_json::to_string_pretty(&data)?;
    std::fs::write(data_path(root), json).context("Failed to write conversations file")?;
    Ok(())
}

pub fn load(root: &Path) -> Result<Vec<Conversation>> {
    let path = data_path(root);
    if !path.exists() {
        anyhow::bail!("No saved conversations");
    }
    let json = std::fs::read_to_string(&path).context("Failed to read conversations file")?;
    let data: SaveData = serde_json::from_str(&json).context("Failed to parse conversations")?;
    Ok(data.conversations)
}
