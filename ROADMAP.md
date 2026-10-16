# Wid Roadmap

> **Last updated:** 2026-10-16
> **Status:** v0.4 in progress — theme system, command palette, menu bar toggle, light mode, and mouse selection shipped; font system (Phase 2), custom title bar (Phase 3), and Phase 5 residuals still open

# Problems & Ideas

## To be implemented
 - Live disk reload/updates
 - Delete buttons for notes (see "UI&UX Ideas")
 - Customizable keybinds

## Lack of Clipboard support and mouse support
 - Support clipboard includes texts and medias
 - should support `yank` and `paste` operations
 - should support mouse operations
 - Mouse wheel scrolling

## Bugs
 - Opening workspaces crashes 
    - log: ```thread 'main' (628261) panicked at /nix/store/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-vendor-cargo-deps/c19b7c6f923b580ac259164a89f2577984ad5ab09ee9d583b888f934adbbe8d0/zbus-5.19.0/src/abstractions/executor.rs:217:9:
     there is no reactor running, must be called from the context of a Tokio 1.x runtime
     note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace```
 - editor isn't functional now. [SOLVED]
 - VIM binding not functioning correctly
    - right now `e` it stops at last letter rather than skipping to next word
    - when using `a` after `o` , `a` will behave like `o` (it skip to insert the next line)
    - double `d` should allow deleting current line [SOLVED]
    
## UI Problems
 - When the windows is small everything gets cluttered up

## Whiteboards
 - The Whiteboard tab switches the main content area from notes to a whiteboard canvas.
 - The Whiteboard tab itself acts as the control/tool panel for interacting with the canvas.
 - Whiteboard-specific tools and controls should live within the tab rather than taking up space on the canvas.
 
## UI&UX Ideas
 - Should allow user to switch from Stylix 
 - holding shift and will reveal delete buttons on notes

## Sidebar
 - Sidebar should be on both left and right side (similar to that of Jetbrains IDEs or Android Studio)
 - Icon should be a 1:1 
 - possibly customizable
 - allows pane to render over the other

## Find solution to lack of other language unicode supports
  - need shipped fonts decision
  - options for custom fonts
 
---

## Session Summary

The following roadmap was defined through an interactive planning session covering three major feature areas for Wid's v0.4 release. The discussion built on the existing v0.1 MVP (vim modal editing, workspace system, markdown preview) and ARCHITECTURE.md vision, adding a concrete plan for a UI overhaul inspired by the Zed editor.

### Key decisions made

| Topic | Decision |
|-------|----------|
| **UI layout** | Topbarless, boxy style — menu bar hidden by default, toggled with bare `Alt` (Firefox-style) or View menu; inline status; optional custom title bar (user-configurable) |
| **Theme file format** | **TOML** — already a dependency, human-readable, supports comments |
| **Theme cascade** | Built-in default → `~/.config/wid/theme.toml` (global) → `<workspace>/theme.toml` (per-workspace override) |
| **Bundled font** | Iosevka Nerd Font (Regular / Medium / Bold variants) |
| **Font configuration** | User-configurable via `settings.toml` — family, weight, size, custom paths |
| **Command palette triggers** | `:` (ex prefix), `Ctrl+P`, `Ctrl+Shift+P` — all rebindable via future key mapping page |
| **`:` behavior** | Opens palette instead of bare command line; typed ex commands (`:w`, `:wq`) show sub-variants + descriptions + keybinding hints in filtered list |
| **Title bar** | Optional custom title bar (hide OS chrome via `decorated = false`) with minimize/maximize/close buttons + drag region; user can toggle on/off |
| **Fuzzy search** | Use `fuzzy-matcher` crate for command palette filtering |

---

## v0.4 — UI Refresh & Command Palette

A comprehensive UI overhaul bringing Wid closer to the Zed editor's aesthetic: clean, boxy, content-focused. The theme and font systems make the app deeply customizable, while the command palette unifies vim ex commands and app-specific actions under a single fuzzy-searchable interface.

### Overview

```
┌─────────────────────────────────────────────────────┐
│ Custom Title Bar (optional, user-configurable)      │
├─────────────────────────────────────────────────────┤
│ Menu bar (hidden by default, Alt to reveal)         │
├──────────┬──────────────────────────────────────────┤
│          │                                          │
│ Sidebar  │          Editor Area                     │
│ (pages,  │   (vim buffer + cursor painting)         │
│ journals)│                                          │
│          │          ┌──────────────────┐            │
│          │          │ Command Palette  │            │
│          │          │ floating overlay │            │
│          │          └──────────────────┘            │
│          │                              ┌──────────┐│
│          │                              │ Preview  ││
│          │                              │ (toggle) ││
│          │                              └──────────┘│
├──────────┴──────────────────────────────────────────┤
│ Inline status (sidebar bottom or editor thin bar)   │
└─────────────────────────────────────────────────────┘
```

---

## Files to Create

| File | Purpose | Details |
|------|---------|---------|
| `src/theme.rs` | Theme system | `Theme` struct, `Default` for built-in dark theme, loading from TOML with cascade (builtin → global → workspace), partial merge |
| `src/fonts.rs` | Font registry | Bundle Iosevka NF (Regular/Medium/Bold `.ttf`), register with egui `FontDefinitions`, parse custom paths from settings |
| `src/command_palette.rs` | Command palette | `CommandPalette` struct, fuzzy filtering, action registry, rendering as floating overlay, keyboard navigation |
| `src/titlebar.rs` | Custom title bar | Title bar widget — minimize/maximize/close buttons, drag region, styling via theme |
| `assets/IosevkaNerdFont-Regular.ttf` | Bundled font | Regular weight of Iosevka Nerd Font |
| `assets/IosevkaNerdFont-Medium.ttf` | Bundled font | Medium weight of Iosevka Nerd Font |
| `assets/IosevkaNerdFont-Bold.ttf` | Bundled font | Bold weight of Iosevka Nerd Font |

---

## Files to Modify

| File | Changes Required |
|------|-----------------|
| `src/settings.rs` | Add font config section (`family`, `size`, `weight` per editor/ui), flat `window.titlebar_*` keys (`titlebar_height`, `titlebar_buttons`, `titlebar_drag`), `window.decorated`, `window.menu_bar` (`"hidden"` default / `"visible"`) |
| `src/app.rs` | Menu bar becomes hidden by default and toggleable (bare `Alt` reveals it, Firefox-style; also a View menu toggle). Add command palette rendering. Add conditional custom title bar. Inline status into sidebar. Wire keyboard shortcuts for palette (`Ctrl+P`, `Ctrl+Shift+P`). |
| `src/widget.rs` | Replace all hardcoded `Color32` values with `theme.editor.*` fields. Use configured font from font registry instead of `FontId::monospace(16.0)`. |
| `src/vim/mod.rs` | Extend `CommandAction` enum with new variants: `TogglePreview`, `ToggleSidebar`, `CreatePage`, `DeletePage`, `OpenNote(String)`, `SearchTags(String)`, `SwitchTheme(String)`, `OpenWorkspace`, `TodayJournal`. Handle `:` key to open palette (not bare command-line mode). Add `ToggleMenuBar` variant. |
| `src/workspace/mod.rs` | Add method to load workspace-level `theme.toml` override. |
| `src/main.rs` | Load global theme from `~/.config/wid/theme.toml`. Register fonts with egui context. Set `decorated` from settings. Apply visuals from theme. |
| `Cargo.toml` | Add `fuzzy-matcher` (fuzzy scoring for command palette). Global config path resolved via existing `src/platform.rs::config_dir()` — no `dirs` crate needed. |

---

## Detailed Execution Plan

### Phase 1 — Theme System (foundation)

**Goal:** All colors and visual properties centralized into a `Theme` struct, no hardcoded `Color32` values in UI code.

**Steps:**

1. Create `src/theme.rs` with:
   - `Theme` struct containing all color groups
   - `Default` impl providing the built-in dark theme
   - `load_global()` → reads `~/.config/wid/theme.toml`
   - `load_workspace(path)` → reads `<workspace>/theme.toml`
   - Merging logic: fields present in file override defaults, missing fields keep defaults
   - Serialization/deserialization via `serde` + `toml`

2. Define color groups:
   ```rust
   pub struct EditorColors {
       pub background: Color32,
       pub foreground: Color32,
       pub cursor: Color32,
       pub cursor_insert: Color32,
       pub cursor_width: f32,
       pub selection_bg: Color32,
       pub line_numbers: Color32,
       pub line_numbers_active: Color32,
       pub gutter_bg: Color32,
       pub whitespace: Color32,
   }

   pub struct SidebarColors { ... }
   pub struct PaletteColors { ... }
   pub struct TitlebarColors { ... }
   pub struct StatusColors { ... }
   pub struct SyntaxColors { ... }  // reserved for future use
   ```

3. Store `Theme` in `WidApp` struct, load once during initialization.

4. Refactor `widget.rs` — pass `&Theme` to `paint_buffer()` and `editor_ui()`, replace `Color32::from_rgb(...)` literals.

5. Refactor `app.rs` — use theme colors for sidebar, status, command line rendering.

**Verification:** App launches with identical visual appearance (dark theme), proving the default matches old hardcoded values.

---

### Phase 2 — Font System

**Goal:** Iosevka NF bundled and configurable; user can switch family, weight, size, or add custom fonts.

**Steps:**

1. Download Iosevka Nerd Font `.ttf` files for Regular, Medium, Bold weights (subset to ~500KB each).

2. Place in `assets/` directory.

3. Create `src/fonts.rs`:
   ```rust
   pub struct FontRegistry {
       pub editor_font: FontId,
       pub ui_font: FontId,
   }

   impl FontRegistry {
       pub fn new(ctx: &egui::Context, settings: &Settings) -> Self;
       pub fn reload(ctx: &egui::Context, settings: &Settings);
   }
   ```

4. Register fonts via `egui::FontDefinitions::append()` or `egui::Context::add_font()`.

5. Add to `settings.toml`:
   ```toml
   [editor.font]
   family = "Iosevka Nerd Font"
   size = 14
   weight = "Medium"

   [ui.font]
   family = "Iosevka Nerd Font"
   size = 13
   weight = "Regular"

   [fonts.custom]
   # path = "C:/Fonts/MyFont.ttf"
   # family = "My Font"
   ```

6. Update `widget.rs` to use `font_registry.editor_font` instead of `FontId::monospace(16.0)`.

**Verification:** App renders editor text in Iosevka NF; changing font settings in `settings.toml` and restarting reflects new font.

---

### Phase 3 — Custom Title Bar

**Goal:** Optional custom title bar replacing OS chrome when `decorated = false`.

**Steps:**

1. Create `src/titlebar.rs`:
   ```rust
   pub struct TitleBar;

   impl TitleBar {
       pub fn ui(ui: &mut egui::Ui, theme: &Theme, frame: &mut eframe::Frame) {
           // Render thin bar at top
           // Draw app name (left)
           // Draw min/max/close buttons (right)
           // Handle drag region
       }
   }
   ```

2. Add to `settings.toml`:
   ```toml
   [window]
   decorated = false
   titlebar_height = 32.0
   titlebar_buttons = true
   titlebar_drag = true
   ```

3. In `main.rs`, conditionally set `ViewportBuilder::with_decorations(false)` based on `settings.window.decorated`.

4. In `app.rs`, when `decorated = false`, render `TitleBar::ui()` at the top of the layout before sidebar/editor.

5. Title bar height should be subtracted from available space for the rest of the layout.

6. Close button dispatches `ViewportCommand::Close`. Minimize dispatches `ViewportCommand::Minimize`. Maximize toggles `ViewportCommand::Maximized`.

**Verification:** With `decorated = false`, app launches without OS title bar; custom bar renders with working buttons and drag.

---

### Phase 4 — Command Palette

**Goal:** Unified fuzzy-searchable action panel replacing the bare command-line mode for `:`.

**Steps:**

1. Create `src/command_palette.rs`:

   ```rust
   pub struct PaletteItem {
       pub id: &'static str,        // unique identifier
       pub label: &'static str,     // display name (e.g., "Save")
        pub keys: &'static [&'static str], // triggers (e.g. `[":w", ":write"]`)
       pub description: &'static str,
       pub keybind: Option<&'static str>,  // e.g., "Ctrl+S"
       pub category: PaletteCategory,
       pub action: CommandAction,
   }

   pub enum PaletteCategory {
       Ex,     // ex-style commands
       App,    // app-specific actions
   }

   pub struct CommandPalette {
       pub visible: bool,
       pub mode: PaletteMode,   // Commands (registry) or Pages (Ctrl+P page finder)
       input: String,
       items: Vec<PaletteItem>,
       filtered: Vec<usize>,
       selected_index: usize,
       pages: Vec<String>,      // workspace page names, refreshed while visible
   }
   ```

2. Implement item registry — a static list of all available commands.

3. Implement fuzzy filtering:
   - Use `fuzzy-matcher` crate's `FuzzyMatcher` for scoring
   - If input starts with `:`, strip `:` and match against `keys` field (ex commands)
   - Otherwise, match against `label` field (app commands)
   - Sort results by score descending

4. Implement keyboard navigation:
   - `Up/Down` arrows → change `selected_index`
   - `Enter` → execute selected action (via returned `CommandAction`)
   - `Escape` → dismiss palette
   - `Backspace` → edit input
   - Character input → filter list

5. Command action registry (complete list):

   | ID | Label | Ex Keys | Description | Default Keybind |
   |----|-------|---------|-------------|-----------------|
   | `save` | Save | `:w`, `:write` | Write buffer to disk | `Ctrl+S` |
   | `quit` | Quit | `:q`, `:quit` | Quit Wid | — |
   | `save_quit` | Save & Quit | `:wq`, `:x` | Save and quit | — |
   | `force_quit` | Force Quit | `:q!`, `:quit!` | Quit without saving | — |
   | `force_save_quit` | Force Save & Quit | `:wq!` | Force save and quit | — |
   | `toggle_preview` | Toggle Preview | — | Show/hide markdown preview | — |
   | `toggle_sidebar` | Toggle Sidebar | — | Show/hide sidebar | — |
   | `create_page` | Create Note | — | Create a new page | — |
   | `delete_page` | Delete Page | — | Delete current page | — |
   | `open_note` | Open Note | `:e`, `:open` | Fuzzy-find and switch pages | `Ctrl+P` |
   | `search_tags` | Search Tags | — | Search by `#tag` | — |
   | `switch_theme` | Switch Theme | — | Cycle or select a theme | — |
   | `open_workspace` | Open Workspace | — | Open a different workspace | — |
   | `today_journal` | Today's Journal | — | Jump to today's journal | — |
   | `help` | Help | `:help` | Display help buffer | — |
   | `clear_search` | Clear Search | `:noh`, `:nohlsearch` | Clear search highlights | — |
   | `zen_mode` | Zen Mode | — | Fullscreen editor only | `F11` |
   | `toggle_menu_bar` | Toggle Menu Bar | `:menubar` | Show/hide the menu bar | `Alt` |

6. Extend `CommandAction` enum in `vim/mod.rs`:
   ```rust
   pub enum CommandAction {
       None,
       Save, Quit, SaveQuit, ForceQuit, SaveForceQuit,
       Open(String),
       TogglePreview,
       ToggleSidebar,
       CreatePage,
       DeletePage,
       OpenNote,
       SearchTags,
       SwitchTheme(String),
       OpenWorkspace,
       TodayJournal,
       SetZenMode(bool),
       ToggleMenuBar,
       /// `:` pressed — open the command palette with an ex prefix.
       OpenPalette,
       /// Run a raw ex command with no dedicated variant (`:help`, `:noh`).
       Ex(String),
   }
   ```
   - `OpenPalette` carries the `:` press out of vim normal mode without keeping `Mode::CommandLine` around.
   - `Ex(String)` covers palette rows `help` and `clear_search` from the table above — no dedicated variants needed.

7. Integrate into `app.rs`:
   - Add `CommandPalette` field to `WidApp`
   - In `render_workspace()`, if `palette.visible`, render overlay after all panels
   - On `:` press: open palette in **Commands** mode with `input = ":"` (ex prefix preselected)
   - On `Ctrl+P` press: open palette in **Pages** mode — fuzzy-find workspace pages/journals
   - On `Ctrl+Shift+P` press: open palette in **Commands** mode with `input = ""` (full command list)
   - Palette captures keyboard when visible (prevent from reaching vim)
   - On action selection: dispatch via `CommandAction` (using existing polling pattern in `render_workspace`)

8. Handle `:` in vim's normal mode:
   - Instead of setting `Mode::CommandLine`, set palette visible with `:` prefix

9. Keep backward compatibility: if user types a complete ex command without selecting from list, execute it directly.

**Verification:** Pressing `:` opens floating command palette in Commands mode; typing `w` filters to `:w`, `:write`, `:wq`, `:wq!`; pressing `Ctrl+P` opens the Pages mode page finder; pressing `Ctrl+Shift+P` lists all commands. Selecting an item executes the action.

---

### Phase 5 — Layout Cleanup

**Goal:** Menu bar hidden by default (revealed on demand), status bar removed, finalize the Zed-inspired layout.

**Steps:**

1. Make the menu bar toggleable instead of deleting it:
   - Default state: hidden (`settings.window.menu_bar = "hidden"`).
   - Pressing bare `Alt` (no chord key) reveals it — Firefox-style — and moves focus to the first menu item; pressing `Alt` again or `Escape` hides it.
   - A `View` menu toggle and the command palette (`ToggleMenuBar`) also flip it.
   - Bare-`Alt` detection must fire only when no chord key follows — existing `Alt+1`, `Alt+2`, `Alt+T` bindings (`src/app.rs:477-484`) must keep working unchanged.

2. Remove `Panel::bottom("status_bar")` block from `app.rs`.

3. Move status information:
   - Workspace name → sidebar header
   - Status messages → thin inline text at bottom of sidebar or as temporary toast
   - Mode indicator → editor area top-left corner or as a thin colored stripe

4. Recalculate layout geometry:
   - If custom title bar is active, subtract its height from `available_rect`
   - Sidebar takes left portion
   - Editor takes center
   - Preview (if open) takes right portion

5. Apply boxy styling:
   ```rust
   let mut style = (*ctx.style()).clone();
   style.visuals.window_rounding = 0.0;
   style.visuals.window_shadow = Default::default();
   style.visuals.popup_shadow = Default::default();
   style.spacing.item_spacing = Vec2::new(0.0, 0.0);
   ctx.set_style(style);
   ```

6. Ensure all panels have no visible frames/strokes — just flat colored rectangles.

**Verification:** App launches with clean, flat, boxy appearance. Menu bar hidden by default and revealed with bare `Alt`; status is readable but unobtrusive.

---

## Future Considerations (beyond v0.4)

- **Key mapping page:** A settings panel where users can rebind all palette triggers, vim keys, and app actions. Stored as a mapping file alongside settings.
- **Multiple themes:** Ship with a light theme + dark theme; allow users to create and share theme files.
- **Syntax highlighting:** Extend `SyntaxColors` in theme to power a syntax highlighter in the editor.
- **command palette extensibility:** Allow agents/plugins to register their own palette items.
- **Font preview:** Show rendered font samples in settings.

---

## Appendix A: Sample `theme.toml`

```toml
[meta]
name = "Wid Dark"
author = "default"

[editor]
background = "#1e1e1e"
foreground = "#d4d4d4"
cursor = "#ffcc00"
cursor_insert = "#ffcc00"
cursor_width = 2
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

## Appendix B: Sample `settings.toml` additions

```toml
[window]
decorated = false
titlebar_height = 32.0
titlebar_buttons = true
titlebar_drag = true
menu_bar = "hidden"   # "hidden" (Alt to reveal, Firefox-style) | "visible"

[editor.font]
family = "Iosevka Nerd Font"
size = 14
weight = "Medium"
ligatures = true

[ui.font]
family = "Iosevka Nerd Font"
size = 13
weight = "Regular"

[fonts.custom]
# Uncomment to add custom fonts:
# path = "C:\\Users\\user\\Fonts\\Custom.ttf"
# family = "Custom Font Name"
```

## Appendix C: Theme cascade

```
1. Built-in Dark Theme (hardcoded Default impl)
       ↓
┌──────────────────────────────┐
│  ~/.config/wid/              │  ← Global config directory
│  └── theme.toml              │     (resolved via `platform::config_dir()`)
└──────────────────────────────┘
       ↓
┌──────────────────────────────┐
│  <workspace>/                │  ← Per-workspace override
│  └── theme.toml              │     (optional, merged on top)
└──────────────────────────────┘
```

Partial files are allowed — missing fields fall through to the next level in the cascade. A workspace `theme.toml` with only `[editor]` changes only editor colors; everything else comes from the global or built-in default.

## Appendix D: Command palette overlay layout

```
┌──────────────────────────────────────────────┐
│  : w                                         │  ← Input field
│  ──────────────────────────────────────────  │
│  :w      Write             (Ctrl+S)          │  ← Selected item (highlighted)
│  :write  Write buffer to disk                │  ← Filtered list
│  :wq     Save and Quit                       │
│  :wq!    Force Save and Quit                 │
│                                              │
│  4 of 4 matches                              │  ← Status hint
└──────────────────────────────────────────────┘
```

- Width: ~60% of window width (max 600px)
- Height: auto up to ~400px, scrollable
- Position: centered horizontally, ~30% from top vertically
- Rendered as a `Frame` with `fill` from `theme.palette.background` + `border` from `theme.palette.border`
- No window decoration, no shadow (boxy style)
