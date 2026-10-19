# Wid — opencode agent notes

Main project language is English

## Project structure

```
Wid/
├── Cargo.toml
├── AGENTS.md
├── ARCHITECTURE.md
├── ROADMAP.md        — v0.4 plan (theme, fonts, titlebar, command palette, layout)
├── THEMING.md        — theme system documentation
├── README.md
├── .gitignore
├── plans/
│   └── explorer.md    — (empty) plans for file-explorer feature
└── src/
    ├── main.rs          — eframe entry point, CLI --search mode
    ├── app.rs           — main UI layout, panels, menu bar, workspace prompt, AI chat panel
    ├── settings.rs      — settings.toml serde (editor, window, theme, invert, ai)
    ├── theme.rs         — Theme struct, built-ins (wid-dark/gruvbox), TOML cascade, light-mode inversion
    ├── sidebar.rs       — activity bars, left/right side panels, icons, search UI
    ├── session.rs       — last-workspace persistence
    ├── platform.rs      — config_dir() helper (~/.config/wid)
    ├── command_palette.rs — fuzzy palette overlay (Commands/Pages modes, action registry)
    ├── ai/
    │   ├── mod.rs       — module re-exports
    │   ├── types.rs     — ChatMessage, Conversation, AsyncEvent, AiConfig
    │   ├── ollama.rs    — Ollama HTTP client (model list, streaming chat)
    │   ├── search.rs    — Search providers (Brave, SearXNG, DuckDuckGo, Tavily)
    │   ├── persistence.rs — Conversation save/load per workspace (.wid/conversations.json)
    │   └── context.rs   — Workspace note context builder (pages/ + journals/)
    ├── editor/
    │   └── mod.rs       — Buffer, Selection, cursor math, text objects (no egui)
    ├── vim/
    │   ├── mod.rs       — VimState, Mode, Registers, operator-pending, repeat, undo, ex commands
    │   └── key.rs       — KeyPress enum (egui-independent) + egui::Event translation
    ├── widget.rs        — editor_ui(): bridges egui events to VimState, paints buffer
    ├── workspace/
    │   ├── mod.rs       — Workspace open/create, page/journal scan
    │   └── config.rs    — workspace.toml serialization
    └── preview.rs       — pulldown-cmark → egui renderer
```

## Build & run

```powershell
cargo run               # debug
cargo run --release     # release
cargo check             # check compilation only (faster)
wid --search <query>    # CLI search mode
wid --search <query> --model <name>  # CLI search + LLM answer
```

> **NixOS note:** the system `cargo` is a rustup proxy that fails on this machine — run toolchain commands through the flake instead: `nix develop -c cargo check` (also `just check`, `just ci`).

### Cross-platform setup

Project-level configs are platform-agnostic. On **Windows only**, set `LIBRARY_PATH` in user-level Cargo config (`%USERPROFILE%\.cargo\config.toml`):

```toml
[env]
LIBRARY_PATH = "D:\\Tools\\mingw64\\lib"
```

Or add `$env:LIBRARY_PATH = "D:\Tools\mingw64\lib"` to your PowerShell `$PROFILE`.

## Architecture

```
App (app.rs)
├── widget             — egui::Event → KeyPress → VimState::handle_key() → Buffer
├── editor (pure)      — Buffer, Selection, byte-based cursor math
├── vim (pure)         — VimState, Mode, registers, operator-motion composition
├── workspace          — file I/O, workspace.toml, journal creation
├── preview            — pulldown_cmark → egui label rendering
├── theme              — Theme struct, TOML cascade (builtin → ~/.config/wid → workspace), inversion
├── settings           — settings.toml load/save (editor metrics, window, theme, ai)
├── sidebar            — activity bars, left/right panels, icons
├── command_palette    — fuzzy palette overlay (Commands/Pages modes, action registry)
└── ai                 — Ollama client, search providers, conversation persistence, note context
    ├── ollama         — HTTP streaming chat with local LLMs
    ├── search         — Brave, SearXNG, DuckDuckGo, Tavily provider abstraction
    ├── persistence    — per-workspace conversation storage
    └── context         — workspace note context injection for LLM prompts
```

- **editor** and **vim** must never import egui — they are pure data-transform layers
- **widget** is the only layer that translates between egui events and the vim/editor types
- **input** is handled by draining `ctx.input_mut().events`, translating to `KeyPress`, and feeding to `VimState::handle_key()`
- **paint** is done via `egui::Painter` with manual font metrics (no `TextEdit` used)
- **`CommandAction`** is polled at top of `render_workspace()` in app.rs, set during `editor_ui()`
- Visual mode shares same pending-handling logic as normal mode (find/till chars)

## Vim implementation notes

- `Mode`: Normal, Insert, Visual(Charwise|Linewise), CommandLine
- `Pending` enum: AwaitingDouble, AwaitingMotion, AwaitingTextObject, AwaitingFindChar, AwaitingTillChar, AwaitingFindBackChar, AwaitingTillBackChar, AwaitingReplaceChar, AwaitingRegister
- Operators compose with motions: `d{motion}`, `y{motion}`, `c{motion}`, `g~{motion}`, `gu{motion}`, `gU{motion}`, `>{motion}`, `<{motion}`
- Text objects via `i`/`a` prefix: `iw`, `aw`, `ip`, `ap`, `i(`, `a(`, `i"`, `a"`, etc.
- Count accumulates before operator and applies to motion (`3dw`, `d3w` both work)
- Undo via snapshot stack (200 levels), redo on Ctrl+r
- Repeat (`.`) stores last insert text or last ChangeRecord
- Cursor = byte offset. `line_col()` / `byte_from_line_col()` convert.
- All vim operations are composable: count + operator + register + motion/text-object

## Workspace layout

```
workspace/
├── workspace.toml      # serde-serialized config
├── pages/              # user pages (.md)
└── journals/           # daily journals (YYYY_MM_DD.md)
```

- First launch shows a workspace prompt dialog
- On workspace open, today's journal is auto-created and loaded
- Sidebar lists today's journal (always first), then pages, then other journals

## Platform gotchas

### egui-winit swallows Ctrl+V / Ctrl+X / Ctrl+C (and their Shift variants)

`egui-winit` converts the clipboard shortcuts into `egui::Event::Cut/Copy/Paste`
*before* it ever emits `egui::Event::Key`, so an app can never receive a
`Key::V`/`Key::X`/`Key::C` press while Ctrl (Cmd on macOS) is held:

- `egui-winit-0.34.3/src/lib.rs:1011-1027` — the cut/copy/paste check runs first and `return`s
- `:1317 is_paste_command` = `modifiers.command && Key::V` (also matches **Ctrl+Shift+V**)
- `:1305 is_cut_command` = `command && Key::X`, `:1311 is_copy_command` = `command && Key::C`
- if the clipboard is **empty**, egui-winit pushes *nothing at all* (`:1019-1026`), so the
  chord fires intermittently depending on clipboard contents

Consequences:

- A chord on Ctrl+Shift+V (or X/C) is dead in the real app — the Preview panel was
  originally bound to Ctrl+Shift+V and had to move to **Ctrl+Shift+P**. Never bind
  Ctrl+Shift+{V,X,C}.
- `egui_kittest` injects raw `egui::Event::Key` directly, bypassing this conversion, so
  unit tests **cannot** catch the problem — the chord tests passed while the live chord
  did nothing. Verify such chords by hand (or via a rail/menu click path).
- Mouse/rail/menu paths are unaffected.

## Status

### Done

- Overhauled text editing into `editor/`, `vim/`, `workspace/` modules (pure data-transform, no egui imports)
- `editor/mod.rs`: `Buffer`, `Selection`, cursor math, text objects
- `vim/mod.rs`: `VimState`, `Mode`, `Registers`, operator-motion composition, repeat (`.`), undo/redo
- `vim/key.rs`: `KeyPress` enum + `egui::Event` translation
- `vim/mod.rs`: Ex commands parsed inline in `execute_command()`
- `workspace/mod.rs` + `config.rs`: Workspace open/create, page/journal scan, `workspace.toml` serde
- `widget.rs`: `editor_ui()` — event bridge + buffer painting
- `app.rs`: workspace prompt dialog, sidebar with pages/journals, status bar, Ctrl+S save
- `CommandAction` enum for deferred ex-command handling (Save, Quit, Open, etc.)
- Visual mode pending find/till chars now work (f, F, t, T in visual mode extend selection)
- Text objects support `a` (outer) vs `i` (inner): `aw`, `ap`, `a(`, `a"`, etc.
- Backward search (`?pattern`) fixed using `rfind`
- `D` (d$), `C` (c$), `Y` (yy) uppercase operators added
- `J` (join lines) and `~` (toggle case) implemented as repeatable operators
- Undo saved once per insert session (i/I/a/A/o/O) — no per-character snapshots
- Deprecated `Panel::min_width` replaced with `min_size`
- `:help` now saves undo snapshot before overwriting buffer
- `p`/`P` handles linewise register paste on correct lines
- `a_word` (outer word) correctly includes trailing/leading whitespace
- `byte_from_line_col` off-by-one fixed for all lines past first
- Count parsing works for all count-prefixed commands (`3dw`, `5j`)
- Visual mode motions no longer short-circuit operator dispatch (d/y/c/~/></ work)
- `join_lines` safely handles multi-byte chars before newlines
- `x` with count accumulates all deleted chars into one register entry
- `toggle_case_at` handles multi-char case mappings (ß→SS)
- `is_char_boundary` checks added to prevent mid-char panics
- Ctrl modifier stuck-state bug fixed in `key.rs:from_egui` — Escape and all non-character keys now work even with stale modifiers
- Double-event processing eliminated: `Event::Text` is sole source for character input
- Unrecognized events dropped instead of recycled back into input queue
- Insert→Normal mode switch on Escape works reliably after extended editing
- `a_W` (outer WORD) text object implemented — `Buffer::a_big_word()` includes trailing/leading whitespace
- `>>`/`<<` line indent/outdent fixed and repeatable via `.` (was stuck unhandled in `AwaitingDouble`)
- `Indent`/`Outdent` change records stored for operator, visual, and linewise operations
- `ChangeRecord::Outdent` added for `.` repeat support
- Dead code references (`changed_lines`, `Selection::extend`/`is_empty`) confirmed absent and removed from todo
- AI `create_workspace()` now initializes AI client/searcher and loads conversations (was missing)
- AI stream chunks routed by conversation ID — switching chats mid-stream no longer corrupts messages
- CLI `--search` mode loads workspace `settings.toml` by walking up from cwd (was hardcoded defaults)
- AI `max_history` config added — truncates messages sent to model (default 20, configurable 5-100)
- AI "Stop" button cancels active streams via `AtomicBool` flag (no new dependencies)
- AI "Retry" button re-sends last message on stream error
- AI conversation export (Markdown or JSON) via `rfd::FileDialog`
- AI `context.rs` renamed from `logseq.rs` — dead `modified_ago` field removed
- AI persistence `SaveData` gains `version: u32` field for future migration support
- Light mode via channel-wise color inversion — `settings.invert` flips any resolved theme to a light variant (`Theme::inverted()`, `invert_color()`), egui `Visuals` follow (`Alt+T` or View ▸ Theme ▸ Light mode; `Alt+1`/`Alt+2` switch built-in themes); `theme.toml` `invert_excludes` keeps listed colors as-authored
- Hardcoded dark-palette accents (mode colors, command-line strip, AI chat bubbles/roles, markdown headings) now route through `invert_color()` in light mode
- Status-bar vim mode indicator text is themeable — `[status]` gains `mode_normal`/`mode_insert`/`mode_visual`/`mode_command` (hex, inverted with the palette); `app.rs` mode-color match now reads `theme.status.mode_*` instead of hardcoded `Color32::YELLOW`/`LIGHT_GREEN`/etc
- `Settings.theme`/`invert` gain `#[serde(default)]` so older `settings.toml` files no longer silently fall back to defaults
- Menu bar hidden by default with Firefox-style toggle — bare `Alt` press/release reveals it (chords like `Alt+1`/`Alt+2`/`Alt+T` do not), View ▸ Menu bar checkbox and `:menubar` ex command (`CommandAction::ToggleMenuBar`) also flip it; startup state from `settings.window.menu_bar` (`"hidden"` default / `"visible"`, `#[serde(default)]`)
- v0.4 Phase 4 command palette (`src/command_palette.rs`) — `:` opens palette in Commands mode with `:` prefix (replaces bare command-line mode), `Ctrl+P` pages finder, fuzzy scoring via `fuzzy-matcher`; palette input drained at top of `WidApp::ui()` before the editor; unmatched ex input (`:e foo`) falls back to `VimState::execute_command`; zen mode (`F11`/palette) hides activity bars + menu and goes fullscreen; deviations from ROADMAP: added `CommandAction::OpenPalette` and `Ex(String)` variants, `Ctrl+P` is the pages finder not a generic picker, **the palette's `Ctrl+Shift+P` (full command list) trigger was dropped** — the Preview panel chord took that key because egui-winit swallows Ctrl+Shift+V (see Platform gotchas); the command list remains reachable via `:`; 9 unit tests in `src/command_palette.rs` cover ex/label filtering, selection clamping, raw-ex fallback, page finder, zen toggle, registry completeness
- Editor keyboard input unblocked — `editor_ui()` now routes keys to vim when `!ctx.text_edit_focused()` instead of `response.has_focus()` (`src/widget.rs:36`); the old focus gate died three ways: egui surrenders focus on every Escape (`EventFilter.escape` defaults to false, so `i`→`Esc` killed all input), the editor's auto-generated widget id shifts when the command-line strip appears (orphaning focus), and nothing focused at startup; 6 egui_kittest regression tests in `src/app.rs:2093` (`editor_input_repro`) drive real pointer/key events through `WidApp::ui`
- Mouse selection — drag in the editor selects text and enters Visual (charwise) mode, plain click moves the cursor / clears the selection / returns to Normal; hit-testing (`pos_to_byte`) shares the painter's monospace grid and cursor-tracked scroll (`view_scroll`); drag anchors use `pointer.press_origin()` (not `interact_pointer_pos`, which tracks the live drag position); 2 more `editor_input_repro` tests cover drag-select and click-to-clear
- Panel keybinds (Zed-style) — `Ctrl+Shift+E` Explorer, `Ctrl+Shift+F` Search, `Ctrl+Shift+P` Preview (was `V`, blocked by egui-winit), `Ctrl+Shift+/` Agent; first press shows the panel (focus stays on editor), second press focuses the panel (vim `j`/`k`/`Enter`/`Escape` navigation), `Ctrl+Q` closes only the focused panel; Escape returns focus to editor but leaves panels open; Explorer/Search rows are keyboard-navigable with cursor reveal/scroll; right panels support `j`/`k` scrolling while focused; `Focus` enum (`Editor/Sidebar/Right`) + `reconcile_focus()` added; `CommandAction` gains `ShowExplorer/ShowSearch/ShowPreview/ShowAgent`; `nav_row` and `Sidebar` gain keyboard cursor state; 8 `panel_keys_repro` kittest tests added.
- Panel chords unblocked while a text field is focused — the chord block (and `Ctrl+Q`) used to sit behind `!ctx.text_edit_focused()`, so focusing the Search field silently killed `Ctrl+Shift+P/Q//` while the ungated legacy `Ctrl+Shift+A` kept working; chords now run whenever `!palette.visible` (only single-letter nav stays text-edit-gated), `Ctrl+Shift+F` auto-focuses the query field (`search_input_id()` + `set_search_input_focus()`), `i` in the focused Search panel hands the keyboard to the field, `release_hidden_search_focus()` drops stale `TextEdit` focus when the view is swapped away, and the Agent header hint reads `Ctrl+Shift+/` (`Ctrl+Shift+A` remains a legacy toggle alias); 2 `panel_keys_repro` regression tests (`search_autofocus_keeps_panel_chords_alive`, `search_i_refocuses_field_and_ctrl_q_still_closes`) bring the suite to 180.
- Search `i` no longer types an `i` — winit commits the `i` keystroke as `Event::Text` alongside (or one frame after) the key press, and focusing the field on the press frame let it swallow that text; the focus is now deferred one frame via `pending_search_focus`, and the pending commit is dropped before the handover (kittest `step()` runs one frame per queued event, which is what exposed the split and forced the one-frame deferral); `search_i_focuses_field_without_typing_an_i` adds the full winit key-down/text/key-up triple and brings the suite to 181.

### Todo

- Panels should focus on first open — pressing a panel chord should reveal *and* focus the panel in one press (today the first press only reveals, so the chord must be pressed twice); needs a decision on how this interacts with re-pressing the chord (release to editor?) and with the Search query-field auto-focus
- v0.4 Phase 2 — font system: `src/fonts.rs` (FontRegistry), bundled Iosevka NF in `assets/`, `[editor.font]`/`[ui.font]`/`[fonts.custom]` settings, replace `FontId::monospace(16.0)` in `widget.rs` (see ROADMAP.md)
- v0.4 Phase 3 — custom title bar: `src/titlebar.rs`, `[window] decorated/titlebar_*` settings
- v0.4 Phase 5 residuals — remove `Panel::bottom("status_bar")` (`src/app.rs`), inline status everywhere, boxy styling (ROADMAP.md:379-387)
- Fix theme cascade merge — `Theme::replace()` resets missing sections to defaults instead of falling through to global/builtin (ROADMAP.md:496)
- Date format TBD for journal filenames
- File explorer feature — plans in `plans/explorer.md` (currently empty)

## Dependencies

- `chrono` — journal date generation
- `serde` + `toml` — workspace.toml config, settings
- `rfd` — native file picker for "Open Workspace"
- `eframe` / `egui` — windowing + immediate-mode GUI
- `pulldown-cmark` — markdown preview rendering
- `reqwest` — HTTP client for Ollama and search providers
- `tokio` — async runtime for streaming LLM responses
- `futures-util` — async stream utilities
- `anyhow` — error handling for AI persistence
- `serde_json` — conversation persistence
- `fuzzy-matcher` — fuzzy scoring for the command palette
