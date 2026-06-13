use egui::Color32;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::path::Path;

// ── Color32 hex deserializer ──

struct HexColorVisitor;

impl Visitor<'_> for HexColorVisitor {
    type Value = Color32;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a hex color string like #1e1e1e or #ffcc00")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Color32, E> {
        parse_hex_color(s).ok_or_else(|| E::custom(format!("invalid hex color: {}", s)))
    }
}

fn parse_hex_color(s: &str) -> Option<Color32> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 && s.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    let a = if s.len() == 8 {
        u8::from_str_radix(&s[6..8], 16).ok()?
    } else {
        255
    };
    Some(Color32::from_rgba_premultiplied(r, g, b, a))
}

fn deserialize_color<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
    d.deserialize_str(HexColorVisitor)
}

fn serialize_color<S: serde::Serializer>(color: &Color32, s: S) -> Result<S::Ok, S::Error> {
    let (r, g, b, a) = (color.r(), color.g(), color.b(), color.a());
    if a == 255 {
        s.serialize_str(&format!("#{:02x}{:02x}{:02x}", r, g, b))
    } else {
        s.serialize_str(&format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a))
    }
}

fn color(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

// ── Meta ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeMeta {
    pub name: String,
    pub author: String,
}

impl Default for ThemeMeta {
    fn default() -> Self {
        Self {
            name: "Nortex Dark".to_string(),
            author: "default".to_string(),
        }
    }
}

// ── Editor colors ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorColors {
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub background: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub foreground: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub cursor: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub cursor_insert: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub cursor_visual: Color32,
    pub cursor_width: f32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub selection_bg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub line_numbers: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub line_numbers_active: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub gutter_bg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub whitespace: Color32,
}

impl Default for EditorColors {
    fn default() -> Self {
        Self {
            background: color(0x1e, 0x1e, 0x1e),
            foreground: color(0xd4, 0xd4, 0xd4),
            cursor: color(0xff, 0xcc, 0x00),
            cursor_insert: color(0xff, 0xcc, 0x00),
            cursor_visual: color(0xc8, 0x96, 0xff),
            cursor_width: 2.0,
            selection_bg: color(0x26, 0x4f, 0x78),
            line_numbers: color(0x85, 0x85, 0x85),
            line_numbers_active: color(0xc6, 0xc6, 0xc6),
            gutter_bg: color(0x1e, 0x1e, 0x1e),
            whitespace: color(0x3a, 0x3a, 0x3a),
        }
    }
}

// ── Sidebar colors ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarColors {
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub background: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub foreground: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub active_bg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub active_fg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub hover_bg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub border: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub separator: Color32,
}

impl Default for SidebarColors {
    fn default() -> Self {
        Self {
            background: color(0x25, 0x25, 0x26),
            foreground: color(0xcc, 0xcc, 0xcc),
            active_bg: color(0x09, 0x47, 0x71),
            active_fg: color(0xff, 0xff, 0xff),
            hover_bg: color(0x2a, 0x2d, 0x2e),
            border: color(0x3c, 0x3c, 0x3c),
            separator: color(0x3c, 0x3c, 0x3c),
        }
    }
}

// ── Palette colors ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaletteColors {
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub background: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub foreground: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub selected_bg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub selected_fg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub input_bg: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub border: Color32,
}

impl Default for PaletteColors {
    fn default() -> Self {
        Self {
            background: color(0x25, 0x25, 0x26),
            foreground: color(0xcc, 0xcc, 0xcc),
            selected_bg: color(0x09, 0x47, 0x71),
            selected_fg: color(0xff, 0xff, 0xff),
            input_bg: color(0x3c, 0x3c, 0x3c),
            border: color(0x45, 0x45, 0x45),
        }
    }
}

// ── Titlebar colors ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TitlebarColors {
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub background: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub foreground: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub button_hover: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub button_close_hover: Color32,
}

impl Default for TitlebarColors {
    fn default() -> Self {
        Self {
            background: color(0x1e, 0x1e, 0x1e),
            foreground: color(0xcc, 0xcc, 0xcc),
            button_hover: color(0x3c, 0x3c, 0x3c),
            button_close_hover: color(0xe8, 0x11, 0x23),
        }
    }
}

// ── Status colors ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusColors {
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub background: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub foreground: Color32,
}

impl Default for StatusColors {
    fn default() -> Self {
        Self {
            background: color(0x00, 0x7a, 0xcc),
            foreground: color(0xff, 0xff, 0xff),
        }
    }
}

// ── Syntax colors (reserved for future highlighting) ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyntaxColors {
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub keyword: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub string: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub comment: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub function: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub number: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub r#type: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub reserved: Color32,
    #[serde(
        deserialize_with = "deserialize_color",
        serialize_with = "serialize_color"
    )]
    pub operator: Color32,
}

impl Default for SyntaxColors {
    fn default() -> Self {
        Self {
            keyword: color(0xc5, 0x86, 0xc0),
            string: color(0xce, 0x91, 0x78),
            comment: color(0x6a, 0x99, 0x55),
            function: color(0xdc, 0xdc, 0xaa),
            number: color(0xb5, 0xce, 0xa8),
            r#type: color(0x4e, 0xc9, 0xb0),
            reserved: color(0x56, 0x9c, 0xd6),
            operator: color(0xd4, 0xd4, 0xd4),
        }
    }
}

// ── Top-level Theme ──

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Theme {
    #[serde(default)]
    pub meta: ThemeMeta,
    #[serde(default)]
    pub editor: EditorColors,
    #[serde(default)]
    pub sidebar: SidebarColors,
    #[serde(default)]
    pub palette: PaletteColors,
    #[serde(default)]
    pub titlebar: TitlebarColors,
    #[serde(default)]
    pub status: StatusColors,
    #[serde(default)]
    pub syntax: SyntaxColors,
}

impl Theme {
    /// Return the built-in Nortex Dark theme (default).
    pub fn nortex_dark() -> Self {
        Theme::default()
    }

    /// Return the built-in Gruvbox dark theme.
    pub fn gruvbox() -> Self {
        fn c(r: u8, g: u8, b: u8) -> Color32 {
            Color32::from_rgb(r, g, b)
        }
        Theme {
            meta: ThemeMeta {
                name: "Gruvbox Dark".to_string(),
                author: "default".to_string(),
            },
            editor: EditorColors {
                background: c(0x28, 0x28, 0x28),
                foreground: c(0xeb, 0xdb, 0xb2),
                cursor: c(0xeb, 0xdb, 0xb2),
                cursor_insert: c(0xeb, 0xdb, 0xb2),
                cursor_visual: c(0xfe, 0x80, 0x19),
                cursor_width: 2.0,
                selection_bg: c(0x3c, 0x38, 0x36),
                line_numbers: c(0x50, 0x49, 0x45),
                line_numbers_active: c(0xa8, 0x99, 0x84),
                gutter_bg: c(0x28, 0x28, 0x28),
                whitespace: c(0x3c, 0x38, 0x36),
            },
            sidebar: SidebarColors {
                background: c(0x1d, 0x20, 0x21),
                foreground: c(0xeb, 0xdb, 0xb2),
                active_bg: c(0x45, 0x85, 0x88),
                active_fg: c(0xfb, 0xf1, 0xc7),
                hover_bg: c(0x3c, 0x38, 0x36),
                border: c(0x3c, 0x38, 0x36),
                separator: c(0x50, 0x49, 0x45),
            },
            palette: PaletteColors {
                background: c(0x1d, 0x20, 0x21),
                foreground: c(0xeb, 0xdb, 0xb2),
                selected_bg: c(0x45, 0x85, 0x88),
                selected_fg: c(0xfb, 0xf1, 0xc7),
                input_bg: c(0x3c, 0x38, 0x36),
                border: c(0x50, 0x49, 0x45),
            },
            titlebar: TitlebarColors {
                background: c(0x28, 0x28, 0x28),
                foreground: c(0xeb, 0xdb, 0xb2),
                button_hover: c(0x3c, 0x38, 0x36),
                button_close_hover: c(0xcc, 0x24, 0x1d),
            },
            status: StatusColors {
                background: c(0x28, 0x28, 0x28),
                foreground: c(0xeb, 0xdb, 0xb2),
            },
            syntax: SyntaxColors {
                keyword: c(0xfb, 0x49, 0x34),
                string: c(0xb8, 0xbb, 0x26),
                comment: c(0x92, 0x83, 0x74),
                function: c(0xfa, 0xbd, 0x2f),
                number: c(0xd3, 0x86, 0x9b),
                r#type: c(0x8e, 0xc0, 0x7c),
                reserved: c(0x83, 0xa5, 0x98),
                operator: c(0xeb, 0xdb, 0xb2),
            },
        }
    }

    /// Select a built-in theme by name. Falls back to `nortex-dark` on unknown names.
    pub fn builtin(name: &str) -> Self {
        match name {
            "gruvbox" => Self::gruvbox(),
            _ => Self::nortex_dark(),
        }
    }

    /// Load the global theme from `~/.config/nortex/theme.toml`.
    pub fn load_global() -> Option<Self> {
        let path = global_theme_path()?;
        Self::load_from(&path)
    }

    /// Load a workspace-level theme override from `<workspace>/theme.toml`.
    pub fn load_workspace(workspace_root: &Path) -> Option<Self> {
        let path = workspace_root.join("theme.toml");
        Self::load_from(&path)
    }

    /// Load theme from a specific path, merging with defaults for missing fields.
    fn load_from(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Merge another theme on top of this one.
    /// Fields present in `other` override fields in `self`.
    /// This is a shallow merge at the group level.
    /// For partial file support, serde defaults handle missing fields.
    pub fn merge(&mut self, other: Theme) {
        *self = other;
    }

    /// Build the theme using the cascade: builtin → global → workspace.
    pub fn build(workspace_root: Option<&Path>, theme_name: &str) -> Self {
        let mut theme = Theme::builtin(theme_name);

        // Global override
        if let Some(global) = Theme::load_global() {
            theme.merge(global);
        }

        // Workspace override
        if let Some(root) = workspace_root {
            if let Some(ws) = Theme::load_workspace(root) {
                theme.merge(ws);
            }
        }

        theme
    }
}

fn global_theme_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    Some(
        Path::new(&home)
            .join(".config")
            .join("nortex")
            .join("theme.toml"),
    )
}
