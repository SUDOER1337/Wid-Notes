# Nortex — opencode agent notes

Main project language is English

## Project structure

```
Nortex/
├── Cargo.toml
├── AGENTS.md
├── ARCHITECTURE.md
├── README.md
├── .gitignore
└── src/
    ├── main.rs          — eframe entry point
    ├── app.rs           — main UI layout, panels, workspace prompt
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
```

### Cross-platform setup

Project-level configs are platform-agnostic. On **Windows only**, set `LIBRARY_PATH` in user-level Cargo config (`%USERPROFILE%\.cargo\config.toml`):

```toml
[env]
LIBRARY_PATH = "D:\\Tools\\mingw64\\lib"
```

Or add `$env:LIBRARY_PATH = "D:\Tools\mingw64\lib"` to your PowerShell `$PROFILE`.

## Architecture

```
App (ui.rs)
├── widget             — egui::Event → KeyPress → VimState::handle_key() → Buffer
├── editor (pure)      — Buffer, Selection, byte-based cursor math
├── vim (pure)         — VimState, Mode, registers, operator-motion composition
├── workspace          — file I/O, workspace.toml, journal creation
└── preview            — pulldown_cmark → egui label rendering
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

### Todo

- Date format TBD for journal filenames

## Dependencies

- `chrono` — journal date generation
- `serde` + `toml` — workspace.toml config
- `rfd` — native file picker for "Open Workspace"
- `eframe` / `egui` — windowing + immediate-mode GUI
- `pulldown-cmark` — markdown preview rendering
