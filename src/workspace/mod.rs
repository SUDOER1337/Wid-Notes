pub mod config;

use crate::settings::Settings;
use config::WorkspaceConfig;
use std::fs;
use std::path::{Path, PathBuf};
// log::warn! / log::info! macros available via crate dependency

pub struct Workspace {
    pub root: PathBuf,
    pub config: WorkspaceConfig,
    pages: Vec<String>,
    journals: Vec<String>,
}

impl Workspace {
    pub fn create(path: &Path, name: &str) -> std::io::Result<Self> {
        fs::create_dir_all(path)?;
        fs::create_dir_all(path.join("pages"))?;
        fs::create_dir_all(path.join("journals"))?;
        fs::create_dir_all(path.join("assets"))?;

        let config = WorkspaceConfig::new(name);
        config.save(&path.join("workspace.toml"))?;

        let mut ws = Self {
            root: path.to_path_buf(),
            config,
            pages: Vec::new(),
            journals: Vec::new(),
        };
        ws.scan();
        Ok(ws)
    }

    pub fn open(path: &Path) -> Option<Self> {
        let config_path = path.join("workspace.toml");
        let config = WorkspaceConfig::load(&config_path)?;

        let mut ws = Self {
            root: path.to_path_buf(),
            config,
            pages: Vec::new(),
            journals: Vec::new(),
        };
        ws.scan();
        Some(ws)
    }

    fn scan(&mut self) {
        self.pages = Self::scan_dir(&self.root.join("pages"), "md");
        self.journals = Self::scan_dir(&self.root.join("journals"), "md");
    }

    fn scan_dir(dir: &Path, ext: &str) -> Vec<String> {
        let mut items = Vec::new();
        match fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(e) => {
                            let path = e.path();
                            if path.extension().is_some_and(|e| e == ext)
                                && let Some(stem) = path.file_stem()
                            {
                                items.push(stem.to_string_lossy().to_string());
                            }
                        }
                        Err(e) => log::warn!("Workspace scan error: {}", e),
                    }
                }
            }
            Err(e) => log::warn!("Workspace scan error reading dir {:?}: {}", dir, e),
        }
        items.sort_by_key(|a| a.to_lowercase());
        items
    }

    pub fn refresh(&mut self) {
        self.scan();
    }

    pub fn pages(&self) -> &[String] {
        &self.pages
    }

    pub fn journals(&self) -> &[String] {
        &self.journals
    }

    pub fn today_journal(&self) -> String {
        chrono::Local::now().format("%Y_%m_%d").to_string()
    }

    pub fn today_journal_path(&self) -> PathBuf {
        let name = self.today_journal();
        self.root.join("journals").join(format!("{}.md", name))
    }

    pub fn ensure_today_journal(&mut self) -> std::io::Result<String> {
        let path = self.today_journal_path();
        if !path.exists() {
            let date = chrono::Local::now().format("%Y-%m-%d %A").to_string();
            let content = format!("# {}\n\n", date);
            fs::write(&path, &content)?;
            self.refresh();
        }
        let name = self.today_journal();
        Ok(name)
    }

    pub fn read_page(&self, name: &str) -> std::io::Result<String> {
        let path = self.page_path(name);
        fs::read_to_string(path)
    }

    pub fn write_page(&self, name: &str, content: &str) -> std::io::Result<()> {
        fs::write(self.resolve_doc_path(name), content)
    }

    pub fn create_page(&mut self, name: &str) -> std::io::Result<String> {
        let path = self.root.join("pages").join(format!("{}.md", name));
        if !path.exists() {
            let content = format!("# {}\n\n", name);
            fs::write(&path, &content)?;
            self.refresh();
        }
        Ok(name.to_string())
    }

    pub fn delete_page(&mut self, name: &str) -> std::io::Result<()> {
        let path = self.resolve_doc_path(name);
        if path.exists() {
            fs::remove_file(path)?;
            self.refresh();
        }
        Ok(())
    }

    pub fn page_path(&self, name: &str) -> PathBuf {
        self.resolve_doc_path(name)
    }

    /// Resolve the filesystem path for a named document.
    /// Journals take lower priority than pages if names collide.
    fn resolve_doc_path(&self, name: &str) -> PathBuf {
        let is_journal =
            self.journals.iter().any(|j| j == name) && !self.pages.iter().any(|p| p == name);
        if is_journal {
            self.root.join("journals").join(format!("{}.md", name))
        } else {
            self.root.join("pages").join(format!("{}.md", name))
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root.join("settings.toml")
    }

    pub fn load_settings(&self) -> Settings {
        Settings::load(&self.settings_path())
    }

    pub fn save_settings(&self, settings: &Settings) -> std::io::Result<()> {
        settings.save(&self.settings_path())
    }
}
