# Nortex Theming System

> **Last updated:** 2026-06-12
> **Status:** Foundation wired into UI, editor uses `theme.editor.*`, Gruvbox built-in, runtime switching via View menu

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Theme Cascade                        │
│                                                         │
│  1. Built-in (selected by settings.theme)               │
│     ├─ "nortex-dark" (Theme::default)                 │
│     └─ "gruvbox"       (Theme::gruvbox)                 │
│       ↓                                                │
│  2. ~/.config/nortex/theme.toml  (global override)    │
│       ↓                                                │
│  3. <workspace>/theme.toml  (workspace override)        │
└─────────────────────────────────────────────────────────┘
```

Each level in the cascade fills in defaults for missing fields. A workspace `theme.toml` that only contains `[editor]` changes only editor colors; everything else inherits from the global or built-in default.

## File Format

Themes are **TOML** files. The format was chosen because:

- Already a dependency (used for `workspace.toml`, `settings.toml`)
- Human-readable and hand-editable
- Supports comments
- No need to parse JSON

### Minimal theme file

```toml
[meta]
name = "My Custom Theme"
author = "you"

[editor]
background = "#1e1e1e"
foreground = "#d4d4d4"
cursor = "#ffcc00"
```

### Full theme file

See [Appendix A](#appendix-a-full-default-theme) for the complete default theme.

---

## Color Groups

### `[meta]` — Theme metadata

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | `"Nortex Dark"` | Display name for theme switcher |
| `author` | string | `"default"` | Author credit |

### `[editor]` — Editor surface

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `background` | hex color | `#1e1e1e` | Editor background |
| `foreground` | hex color | `#d4d4d4` | Default text color |
| `cursor` | hex color | `#ffcc00` | Cursor block (Normal/Visual mode) |
| `cursor_insert` | hex color | `#ffcc00` | Cursor bar (Insert mode) |
| `cursor_visual` | hex color | `#c896ff` | Cursor block (Visual mode) |
| `cursor_width` | float | `2.0` | Insert cursor thickness in pixels |
| `selection_bg` | hex color | `#264f78` | Selection highlight background |
| `line_numbers` | hex color | `#858585` | Line number text |
| `line_numbers_active` | hex color | `#c6c6c6` | Current line number |
| `gutter_bg` | hex color | `#1e1e1e` | Line number gutter background |
| `whitespace` | hex color | `#3a3a3a` | Reserved — invisible character rendering |

### `[sidebar]` — Sidebar panel

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `background` | hex color | `#252526` | Sidebar background |
| `foreground` | hex color | `#cccccc` | Sidebar text |
| `active_bg` | hex color | `#094771` | Active/selected item background |
| `active_fg` | hex color | `#ffffff` | Active/selected item text |
| `hover_bg` | hex color | `#2a2d2e` | Hover state background |
| `border` | hex color | `#3c3c3c` | Panel borders |
| `separator` | hex color | `#3c3c3c` | Section separator lines |

### `[palette]` — Command palette overlay

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `background` | hex color | `#252526` | Palette background |
| `foreground` | hex color | `#cccccc` | Palette text |
| `selected_bg` | hex color | `#094771` | Selected list item background |
| `selected_fg` | hex color | `#ffffff` | Selected list item text |
| `input_bg` | hex color | `#3c3c3c` | Input field background |
| `border` | hex color | `#454545` | Palette border |

### `[titlebar]` — Custom window title bar

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `background` | hex color | `#1e1e1e` | Title bar background |
| `foreground` | hex color | `#cccccc` | Title bar text |
| `button_hover` | hex color | `#3c3c3c` | Window button hover state |
| `button_close_hover` | hex color | `#e81123` | Close button hover state |

### `[status]` — Status bar

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `background` | hex color | `#007acc` | Status bar background |
| `foreground` | hex color | `#ffffff` | Status bar text |

### `[syntax]` — Syntax highlighting (future)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `keyword` | hex color | `#c586c0` | `if`, `else`, `for`, `return` |
| `string` | hex color | `#ce9178` | String literals |
| `comment` | hex color | `#6a9955` | Comments |
| `function` | hex color | `#dcdcaa` | Function names |
| `number` | hex color | `#b5cea8` | Numeric literals |
| `type` | hex color | `#4ec9b0` | Type names |
| `reserved` | hex color | `#569cd6` | Keywords like `use`, `pub`, `mod` |
| `operator` | hex color | `#d4d4d4` | Operators |

---

## Color Format

All colors use 6-digit hex (`#rrggbb`) or 8-digit hex with alpha (`#rrggbbaa`).

Examples:
```toml
background = "#1e1e1e"       # fully opaque dark gray
selection_bg = "#264f7880"   # semi-transparent blue
```

---

## Current Implementation Status

### Done

- `Theme` struct with all 7 color groups
- `Default` impl providing the built-in "Nortex Dark" theme
- `#[serde(default)]` on all groups enables partial file support
- `Theme::builtin(name)` dispatcher — `"nortex-dark"` and `"gruvbox"` built-ins
- `Theme::gruvbox()` — full Gruvbox dark palette across all 7 sections
- `Theme::build(workspace_root, theme_name)` — cascade loader (builtin → global → workspace)
- `Theme::load_global()` — reads `~/.config/nortex/theme.toml`
- `Theme::load_workspace(path)` — reads `<workspace>/theme.toml`
- Custom hex color deserializer (`#rrggbb` and `#rrggbbaa`)
- Custom hex color serializer (round-trips through TOML)
- Global config path resolved via `$HOME/.config/` (no external `dirs` dep)
- `Settings.theme` field persisted in `settings.toml`
- `Theme` field in `NortexApp` struct, built during `new()`/`open_workspace()`/`create_workspace()`
- `editor_ui()` accepts `&Theme`, `paint_buffer()` uses `EditorColors` for all hardcoded editor colors:
  - Editor background, foreground, line numbers, selection background
  - Cursor block (Normal/Visual), cursor bar (Insert), cursor text contrast
  - Hint text color
- `cursor_visual` field on `EditorColors`
- View → Theme → Nortex Dark / Gruvbox Dark radio menu for runtime switching
- `apply_theme()` helper — rebuilds theme and persists settings

### Not yet done

- `app.rs` — sidebar, status bar, palette, titlebar colors use `theme.*` (still hardcoded)
- `:theme` command → `SwitchTheme` action
- Runtime theme reloading (watch file or `:theme reload`)
- `egui::Visuals` derived from theme (dark vs light detection)
- agnoster/statusline mode indicator colored by `mode` values
- `:theme reload` hot-reload from disk
- Theme directory (`~/.config/nortex/themes/*.toml`)
- Syntax highlighting integration (future)

---

## Integration Guide

### Step 1: Add Theme to NortexApp ✅ Done

```rust
pub struct NortexApp {
    pub theme: Theme,            // ← added
    // ...
}
```

Built during `new()` and rebuilt on workspace open/create via `Theme::build(root, &self.settings.theme)`.

### Step 2: Pass Theme to Editor ✅ Done

```rust
pub fn editor_ui(
    ui: &mut egui::Ui,
    state: &mut VimState,
    buffer: &mut Buffer,
    settings: &Settings,
    theme: &Theme,              // ← added
)
```

### Step 3: Replace Hardcoded Colors in paint_buffer() ✅ Done

All 7 hardcoded `Color32::from_rgb(...)` calls replaced with `ec.background`, `ec.foreground`, `ec.line_numbers`, `ec.selection_bg`, `ec.cursor`, `ec.cursor_insert`, `ec.cursor_visual`.

### Step 4: Wire App-level Colors ⬜ Not yet done

| Location | Current hardcoded | Replace with |
|----------|------------------|--------------|
| `app.rs:498` — status bar frame fill | `Color32::from_rgb(20, 20, 30)` | `theme.sidebar.background` or `theme.status.background` |
| `app.rs:432` — "Pages" label | `Color32::GRAY` | `theme.sidebar.foreground` |
| `app.rs:503` — mode indicator | `Color32::YELLOW` / `Color32::LIGHT_GREEN` / etc | Keep mode colors or let theme define them |
| `app.rs:559` — bottom status bar | `Color32::GRAY` | `theme.status.foreground` |
| `app.rs:562` — workspace name | `Color32::GRAY` | `theme.status.foreground` |

### Step 5: Apply Theme to egui Visuals

In `main.rs` after theme is loaded:

```rust
let visuals = if is_dark_theme(&theme) {
    egui::Visuals::dark()
} else {
    egui::Visuals::light()
};
cc.egui_ctx.set_visuals(visuals);
```

With custom overrides:
```rust
let mut style = (*cc.egui_ctx.style()).clone();
style.visuals.window_rounding = 0.0;
style.visuals.window_shadow = Default::default();
// Inherit from theme
style.visuals.window_fill = theme.editor.background;
cc.egui_ctx.set_style(style);
```

---

## Theme Switching at Runtime

Users should be able to switch themes without restarting:

| Method | Implementation |
|--------|---------------|
| `:theme <name>` | Switch to a named theme from a themes directory |
| `:theme reload` | Reload `theme.toml` from disk |
| Command palette | List available themes as palette items |

Themes directory (`~/.config/nortex/themes/`):
```
themes/
├── nortex-dark.toml
├── nortex-light.toml
└── solarized-dark.toml
```

Switching is a matter of calling `Theme::build()` again and calling `ctx.request_repaint()`.

---

## Creating a Light Theme

```toml
[meta]
name = "Nortex Light"
author = "default"

[editor]
background = "#ffffff"
foreground = "#333333"
cursor = "#1a1a1a"
cursor_insert = "#1a1a1a"
selection_bg = "#add6ff"
line_numbers = "#237893"
line_numbers_active = "#1a1a1a"
gutter_bg = "#ffffff"

[sidebar]
background = "#f3f3f3"
foreground = "#333333"
active_bg = "#0078d4"
active_fg = "#ffffff"
hover_bg = "#e8e8e8"
border = "#e0e0e0"
separator = "#e0e0e0"

[palette]
background = "#f3f3f3"
foreground = "#333333"
selected_bg = "#0078d4"
selected_fg = "#ffffff"
input_bg = "#ffffff"
border = "#e0e0e0"

[titlebar]
background = "#ffffff"
foreground = "#333333"
button_hover = "#e0e0e0"
button_close_hover = "#e81123"

[status]
background = "#0078d4"
foreground = "#ffffff"
```

Save to either:
- `~/.config/nortex/themes/nortex-light.toml` (global, available for theme switcher)
- `~/.config/nortex/theme.toml` (global, loads immediately)
- `<workspace>/theme.toml` (per-workspace)

---

## File Locations

| Path | Scope | Loaded |
|------|-------|--------|
| Built-in `Default` impl | Application | Always (fallback) |
| `~/.config/nortex/theme.toml` | Global user | At startup |
| `~/.config/nortex/themes/*.toml` | Global library | On `:theme` command |
| `<workspace>/theme.toml` | Per-workspace | On workspace open |

The global config directory is resolved via `$HOME/.config/nortex/` (Linux) or `%USERPROFILE%\.config\nortex\` (Windows).
- **Linux:** `~/.config/nortex/`
- **Windows:** `%USERPROFILE%\.config\nortex\`
- **macOS:** `~/.config/nortex/`

---

## Appendix A: Full Default Theme

```toml
[meta]
name = "Nortex Dark"
author = "default"

[editor]
background = "#1e1e1e"
foreground = "#d4d4d4"
cursor = "#ffcc00"
cursor_insert = "#ffcc00"
cursor_visual = "#c896ff"
cursor_width = 2.0
selection_bg = "#264f78"
line_numbers = "#858585"
line_numbers_active = "#c6c6c6"
gutter_bg = "#1e1e1e"
whitespace = "#3a3a3a"

[sidebar]
background = "#252526"
foreground = "#cccccc"
active_bg = "#094771"
active_fg = "#ffffff"
hover_bg = "#2a2d2e"
border = "#3c3c3c"
separator = "#3c3c3c"

[palette]
background = "#252526"
foreground = "#cccccc"
selected_bg = "#094771"
selected_fg = "#ffffff"
input_bg = "#3c3c3c"
border = "#454545"

[titlebar]
background = "#1e1e1e"
foreground = "#cccccc"
button_hover = "#3c3c3c"
button_close_hover = "#e81123"

[status]
background = "#007acc"
foreground = "#ffffff"

[syntax]
reserved = "#569cd6"
string = "#ce9178"
comment = "#6a9955"
function = "#dcdcaa"
number = "#b5cea8"
type = "#4ec9b0"
keyword = "#c586c0"
operator = "#d4d4d4"
```

## Appendix B: Hex Color Quick Reference

Common hex colors used in dark themes:

| Color | Hex | Usage |
|-------|-----|-------|
| Dark background | `#1e1e1e` | Editor, title bar |
| Sidebar background | `#252526` | Sidebar |
| Light text | `#cccccc` | Sidebar, palette |
| Bright text | `#d4d4d4` | Editor foreground |
| Yellow | `#ffcc00` | Cursor |
| Selection blue | `#264f78` | Selection |
| Active blue | `#094771` | Active sidebar item |
| Status blue | `#007acc` | Status bar |
| Button red | `#e81123` | Close button hover |
| Gray | `#858585` | Line numbers |
| Dark gray | `#3c3c3c` | Borders, separators |
| Input background | `#3c3c3c` | Palette input |
| Hover | `#2a2d2e` | Sidebar hover |

## Appendix C: Roadmap

| Phase | Description | Status |
|-------|-------------|--------|
| Foundation | Theme struct, hex parser, cascade loader, built-in Gruvbox | ✅ Done |
| Widget integration | Wire theme into `widget.rs` paint_buffer | ✅ Done |
| Runtime switching | View menu with Nortex Dark / Gruvbox Dark toggle, persists to settings | ✅ Done |
| App integration | Wire theme into `app.rs` sidebar/status/titlebar colors | ⬜ Pending |
| Font integration | Bundle Iosevka NF, register with egui | ⬜ Pending |
| Custom titlebar | Render custom title bar when `decorated: false` | ⬜ Pending |
| Command palette | Floating overlay with fuzzy search | ⬜ Pending |
| Theme directory | `~/.config/nortex/themes/*.toml` library with `:theme <name>` | ⬜ Pending |
| Theme reload | `:theme reload` hot-reload from disk | ⬜ Pending |
| Syntax highlighting | Parse markdown AST, apply `theme.syntax.*` | ⬜ Future |
