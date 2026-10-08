use crate::platform;
use egui::Color32;
use serde::de::{self, Error as _, Visitor};
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

fn deserialize_color_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Color32>, D::Error> {
    Vec::<String>::deserialize(d)?
        .iter()
        .map(|s| {
            parse_hex_color(s).ok_or_else(|| D::Error::custom(format!("invalid hex color: {s}")))
        })
        .collect()
}

fn serialize_color_vec<S: serde::Serializer>(colors: &[Color32], s: S) -> Result<S::Ok, S::Error> {
    let strs: Vec<String> = colors
        .iter()
        .map(|c| {
            let (r, g, b, a) = (c.r(), c.g(), c.b(), c.a());
            if a == 255 {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
            }
        })
        .collect();
    strs.serialize(s)
}

fn color(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/** Invert a color channel-wise for a quick light-mode flip of a dark palette.
 *  Alpha is preserved; opaque colors (all theme colors) invert cleanly.
 *  @param c - Color authored for a dark background
 *  @returns The inverted color, readable on a light background
 */
pub fn invert_color(c: Color32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(255 - r, 255 - g, 255 - b, a)
}

/** Invert a color unless it is listed in `excluded`.
 *  Excluded colors keep their authored value, which is how a theme preserves
 *  accent colors that would become unreadable complements after inversion.
 *  @param c - Color authored for a dark background
 *  @param excluded - Colors to leave untouched (empty slices invert everything)
 *  @returns Excluded colors as-is, everything else inverted
 */
pub fn invert_color_if(c: Color32, excluded: &[Color32]) -> Color32 {
    if excluded.contains(&c) {
        c
    } else {
        invert_color(c)
    }
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
            name: "Wid Dark".to_string(),
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

impl EditorColors {
    /// Return a copy with every color inverted (light-mode variant),
    /// skipping colors listed in `excluded`.
    pub fn inverted(&self, excluded: &[Color32]) -> Self {
        Self {
            background: invert_color_if(self.background, excluded),
            foreground: invert_color_if(self.foreground, excluded),
            cursor: invert_color_if(self.cursor, excluded),
            cursor_insert: invert_color_if(self.cursor_insert, excluded),
            cursor_visual: invert_color_if(self.cursor_visual, excluded),
            cursor_width: self.cursor_width,
            selection_bg: invert_color_if(self.selection_bg, excluded),
            line_numbers: invert_color_if(self.line_numbers, excluded),
            line_numbers_active: invert_color_if(self.line_numbers_active, excluded),
            gutter_bg: invert_color_if(self.gutter_bg, excluded),
            whitespace: invert_color_if(self.whitespace, excluded),
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

impl SidebarColors {
    /// Return a copy with every color inverted (light-mode variant),
    /// skipping colors listed in `excluded`.
    pub fn inverted(&self, excluded: &[Color32]) -> Self {
        Self {
            background: invert_color_if(self.background, excluded),
            foreground: invert_color_if(self.foreground, excluded),
            active_bg: invert_color_if(self.active_bg, excluded),
            active_fg: invert_color_if(self.active_fg, excluded),
            hover_bg: invert_color_if(self.hover_bg, excluded),
            border: invert_color_if(self.border, excluded),
            separator: invert_color_if(self.separator, excluded),
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

impl PaletteColors {
    /// Return a copy with every color inverted (light-mode variant),
    /// skipping colors listed in `excluded`.
    pub fn inverted(&self, excluded: &[Color32]) -> Self {
        Self {
            background: invert_color_if(self.background, excluded),
            foreground: invert_color_if(self.foreground, excluded),
            selected_bg: invert_color_if(self.selected_bg, excluded),
            selected_fg: invert_color_if(self.selected_fg, excluded),
            input_bg: invert_color_if(self.input_bg, excluded),
            border: invert_color_if(self.border, excluded),
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

impl TitlebarColors {
    /// Return a copy with every color inverted (light-mode variant),
    /// skipping colors listed in `excluded`.
    pub fn inverted(&self, excluded: &[Color32]) -> Self {
        Self {
            background: invert_color_if(self.background, excluded),
            foreground: invert_color_if(self.foreground, excluded),
            button_hover: invert_color_if(self.button_hover, excluded),
            button_close_hover: invert_color_if(self.button_close_hover, excluded),
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

impl StatusColors {
    /// Return a copy with every color inverted (light-mode variant),
    /// skipping colors listed in `excluded`.
    pub fn inverted(&self, excluded: &[Color32]) -> Self {
        Self {
            background: invert_color_if(self.background, excluded),
            foreground: invert_color_if(self.foreground, excluded),
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

impl SyntaxColors {
    /// Return a copy with every color inverted (light-mode variant),
    /// skipping colors listed in `excluded`.
    pub fn inverted(&self, excluded: &[Color32]) -> Self {
        Self {
            keyword: invert_color_if(self.keyword, excluded),
            string: invert_color_if(self.string, excluded),
            comment: invert_color_if(self.comment, excluded),
            function: invert_color_if(self.function, excluded),
            number: invert_color_if(self.number, excluded),
            r#type: invert_color_if(self.r#type, excluded),
            reserved: invert_color_if(self.reserved, excluded),
            operator: invert_color_if(self.operator, excluded),
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
    /// Colors kept as-authored when light-mode inversion runs.
    /// Painted values equal to an entry here are exempt from `invert_color`.
    #[serde(
        default,
        deserialize_with = "deserialize_color_vec",
        serialize_with = "serialize_color_vec"
    )]
    pub invert_excludes: Vec<Color32>,
}

impl Theme {
    /// Return the built-in Wid Dark theme (default).
    pub fn wid_dark() -> Self {
        Theme::default()
    }

    /// Return the built-in Gruvbox dark theme.
    pub fn gruvbox() -> Self {
        Theme {
            meta: ThemeMeta {
                name: "Gruvbox Dark".to_string(),
                author: "default".to_string(),
            },
            editor: EditorColors {
                background: color(0x28, 0x28, 0x28),
                foreground: color(0xeb, 0xdb, 0xb2),
                cursor: color(0xeb, 0xdb, 0xb2),
                cursor_insert: color(0xeb, 0xdb, 0xb2),
                cursor_visual: color(0xfe, 0x80, 0x19),
                cursor_width: 2.0,
                selection_bg: color(0x3c, 0x38, 0x36),
                line_numbers: color(0x50, 0x49, 0x45),
                line_numbers_active: color(0xa8, 0x99, 0x84),
                gutter_bg: color(0x28, 0x28, 0x28),
                whitespace: color(0x3c, 0x38, 0x36),
            },
            sidebar: SidebarColors {
                background: color(0x1d, 0x20, 0x21),
                foreground: color(0xeb, 0xdb, 0xb2),
                active_bg: color(0x45, 0x85, 0x88),
                active_fg: color(0xfb, 0xf1, 0xc7),
                hover_bg: color(0x3c, 0x38, 0x36),
                border: color(0x3c, 0x38, 0x36),
                separator: color(0x50, 0x49, 0x45),
            },
            palette: PaletteColors {
                background: color(0x1d, 0x20, 0x21),
                foreground: color(0xeb, 0xdb, 0xb2),
                selected_bg: color(0x45, 0x85, 0x88),
                selected_fg: color(0xfb, 0xf1, 0xc7),
                input_bg: color(0x3c, 0x38, 0x36),
                border: color(0x50, 0x49, 0x45),
            },
            titlebar: TitlebarColors {
                background: color(0x28, 0x28, 0x28),
                foreground: color(0xeb, 0xdb, 0xb2),
                button_hover: color(0x3c, 0x38, 0x36),
                button_close_hover: color(0xcc, 0x24, 0x1d),
            },
            status: StatusColors {
                background: color(0x28, 0x28, 0x28),
                foreground: color(0xeb, 0xdb, 0xb2),
            },
            syntax: SyntaxColors {
                keyword: color(0xfb, 0x49, 0x34),
                string: color(0xb8, 0xbb, 0x26),
                comment: color(0x92, 0x83, 0x74),
                function: color(0xfa, 0xbd, 0x2f),
                number: color(0xd3, 0x86, 0x9b),
                r#type: color(0x8e, 0xc0, 0x7c),
                reserved: color(0x83, 0xa5, 0x98),
                operator: color(0xeb, 0xdb, 0xb2),
            },
            invert_excludes: Vec::new(),
        }
    }

    /// Select a built-in theme by name. Falls back to `wid-dark` on unknown names.
    pub fn builtin(name: &str) -> Self {
        match name {
            "gruvbox" => Self::gruvbox(),
            _ => Self::wid_dark(),
        }
    }

    /// Load the global theme from `~/.config/wid/theme.toml`.
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

    /// Replace this theme entirely with `other`.
    /// For partial overrides, serde `#[serde(default)]` on each field group
    /// fills in defaults when individual sections are absent from the file.
    pub fn replace(&mut self, other: Theme) {
        *self = other;
    }

    /// Return a copy of this theme with every color inverted (light-mode variant).
    /// Colors listed in `invert_excludes` keep their authored value.
    pub fn inverted(&self) -> Self {
        let excluded = self.invert_excludes.as_slice();
        Self {
            meta: self.meta.clone(),
            editor: self.editor.inverted(excluded),
            sidebar: self.sidebar.inverted(excluded),
            palette: self.palette.inverted(excluded),
            titlebar: self.titlebar.inverted(excluded),
            status: self.status.inverted(excluded),
            syntax: self.syntax.inverted(excluded),
            invert_excludes: self.invert_excludes.clone(),
        }
    }

    /// Build the theme using the cascade: builtin → global → workspace.
    pub fn build(workspace_root: Option<&Path>, theme_name: &str) -> Self {
        let mut theme = Theme::builtin(theme_name);

        // Global override
        if let Some(global) = Theme::load_global() {
            theme.replace(global);
        }

        // Workspace override
        if let Some(root) = workspace_root {
            if let Some(ws) = Theme::load_workspace(root) {
                theme.replace(ws);
            }
        }

        theme
    }
}

fn global_theme_path() -> Option<std::path::PathBuf> {
    Some(platform::config_dir().join("theme.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /** Inversion flips every channel, preserving alpha.
     *  @param c - opaque: invert twice → identity
     */
    #[test]
    fn invert_color_round_trips_opaque() {
        let c = color(0x1e, 0x2f, 0x40);
        assert_eq!(c, invert_color(invert_color(c)));
        assert_eq!(invert_color(c), color(0xe1, 0xd0, 0xbf));
    }

    /** Primary colors flip to their complements (yellow → blue, etc.).
     *  @param c - saturated accent
     */
    #[test]
    fn invert_color_flips_saturated_hues() {
        assert_eq!(invert_color(Color32::YELLOW), Color32::from_rgb(0, 0, 255));
        assert_eq!(invert_color(Color32::RED), Color32::from_rgb(0, 255, 255));
    }

    /** Excluded colors keep their authored value, everything else inverts.
     *  @param c - accent to preserve
     */
    #[test]
    fn invert_color_if_skips_excluded() {
        let excluded = [Color32::RED];
        assert_eq!(invert_color_if(Color32::RED, &excluded), Color32::RED);
        assert_eq!(
            invert_color_if(Color32::GREEN, &excluded),
            invert_color(Color32::GREEN)
        );
    }

    /** Theme::inverted honors the top-level exclude list.
     *  @param t - theme with an excluded editor background
     */
    #[test]
    fn theme_inverted_respects_excludes() {
        let t = Theme {
            editor: EditorColors {
                background: Color32::RED,
                ..EditorColors::default()
            },
            invert_excludes: vec![Color32::RED],
            ..Theme::default()
        };
        let inv = t.inverted();
        assert_eq!(inv.editor.background, Color32::RED);
        assert_ne!(inv.editor.foreground, t.editor.foreground);
        assert_ne!(inv.editor.cursor, t.editor.cursor);
    }

    /** Theme.toml round-trips hex color excludes.
     *  @param t - theme with excludes
     */
    #[test]
    fn invert_excludes_serde_round_trip() {
        let t = Theme {
            invert_excludes: vec![color(0x00, 0x7a, 0xcc)],
            ..Theme::default()
        };
        let toml_str = toml::to_string(&t).unwrap();
        let back: Theme = toml::from_str(&toml_str).unwrap();
        assert_eq!(t.invert_excludes, back.invert_excludes);
    }
}
