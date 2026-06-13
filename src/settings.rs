use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub editor: EditorSettings,
    pub window: WindowSettings,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            editor: EditorSettings::default(),
            window: WindowSettings::default(),
            theme: "nortex-dark".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EditorSettings {
    pub char_width: f32,
    pub line_height: f32,
    pub padding: f32,
    pub gutter_width: f32,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            char_width: 9.6,
            line_height: 22.0,
            padding: 4.0,
            gutter_width: 48.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowSettings {
    pub width: f32,
    pub height: f32,
    pub min_width: f32,
    pub min_height: f32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            min_width: 600.0,
            min_height: 400.0,
        }
    }
}

impl Settings {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let content = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, content)
    }
}
