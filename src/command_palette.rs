// File: src/command_palette.rs
// Functions:
//   CommandPalette::new()          — build static item registry + empty state
//   open_commands(prefix)          — show palette in command mode (":" or "")
//   open_pages()                   — show palette in page-finder mode (Ctrl+P)
//   close()                        — hide the palette
//   recompute()                    — fuzzy-filter items/pages for current input
//   move_selection(delta)          — move the highlighted row (clamped)
//   execute()                      — run the selected row → PaletteOutcome
//   handle_input(&mut InputState)  — drain keyboard/wheel events, return outcome
//   ui(&Context, &Theme)           — render the floating overlay, return click outcome
//
// Helpers:
//   registry()                     — static PaletteItem list (ROADMAP Phase 4 table)
//   total()                        — unfiltered row count for the current mode
//   render_input() / render_list() / render_status() — overlay sections

use crate::theme::Theme;
use crate::vim::CommandAction;
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// Where a palette entry came from — decides how input text is matched.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PaletteCategory {
    /// Ex-style command (`:w`, `:q`, `:menubar`, …).
    Ex,
    /// App-specific action (no ex prefix).
    App,
}

/// One selectable row in the palette.
pub struct PaletteItem {
    /// Unique identifier, e.g. `"save"`.
    pub id: &'static str,
    /// Display name, e.g. `"Save"` — matched when input has no `:` prefix.
    pub label: &'static str,
    /// Ex triggers (`[":w", ":write"]`) — matched when input starts with `:`.
    pub keys: &'static [&'static str],
    /// One-line help, shown as a hover tooltip.
    pub description: &'static str,
    /// Default shortcut hint, e.g. `"Ctrl+S"`.
    pub keybind: Option<&'static str>,
    pub category: PaletteCategory,
    pub action: CommandAction,
}

/// What the palette is currently filtering over.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PaletteMode {
    /// Command registry (ex commands + app actions).
    Commands,
    /// Workspace page/journal finder (`Ctrl+P`).
    Pages,
}

/// Result of a palette interaction, dispatched by `WidApp`.
pub enum PaletteOutcome {
    /// Run a registered action.
    Action(CommandAction),
    /// Unmatched ex input — execute verbatim through `VimState::execute_command`.
    Raw(String),
    /// Nothing to do (dismissed, or no matches under the cursor).
    None,
}

/// Floating fuzzy-search overlay: command palette (`:`, `Ctrl+Shift+P`)
/// and page finder (`Ctrl+P`).
pub struct CommandPalette {
    pub visible: bool,
    pub mode: PaletteMode,
    /// Raw input text, including the `:` prefix in command mode.
    pub input: String,
    pub items: Vec<PaletteItem>,
    /// Indices into `items` (Commands) or `pages` (Pages), best score first.
    pub filtered: Vec<usize>,
    pub selected_index: usize,
    /// Page/journal names, refreshed by the app while visible.
    pub pages: Vec<String>,
    /// Mirrored from the app so the zen item can toggle correctly.
    pub zen: bool,
    /// Last rendered window rect — routes wheel scrolling.
    pub rect: egui::Rect,
    /// Set when the keyboard moves the selection, to keep the row in view.
    scroll_to_selected: bool,
    matcher: SkimMatcherV2,
}

impl CommandPalette {
    /** Create the palette with the static command registry loaded.
     *  @returns A hidden palette ready to be opened
     */
    pub fn new() -> Self // Build registry and empty state
    {
        Self {
            visible: false,
            mode: PaletteMode::Commands,
            input: String::new(),
            items: registry(),
            filtered: Vec::new(),
            selected_index: 0,
            pages: Vec::new(),
            zen: false,
            rect: egui::Rect::NOTHING,
            scroll_to_selected: false,
            matcher: SkimMatcherV2::default(),
        }
    }

    /** Open the palette in command mode.
     *  @param prefix - `":"` to preselect ex commands, `""` for everything
     */
    pub fn open_commands(&mut self, prefix: &str) // Show command mode with a prefix
    {
        self.visible = true;
        self.mode = PaletteMode::Commands;
        self.input = prefix.to_string();
        self.recompute();
    }

    /** Open the palette in page-finder mode (Ctrl+P).
     *  Uses the last refreshed `pages` list.
     */
    pub fn open_pages(&mut self) // Show page/journal finder
    {
        self.visible = true;
        self.mode = PaletteMode::Pages;
        self.input.clear();
        self.recompute();
    }

    /// Hide the palette and clear its input.
    pub fn close(&mut self) // Dismiss the overlay
    {
        self.visible = false;
        self.input.clear();
        self.filtered.clear();
        self.selected_index = 0;
    }

    /** Re-run the fuzzy filter for the current mode and input.
     *  Rows are rescored and sorted best-first in one pass.
     */
    pub fn recompute(&mut self) // Filter rows for the current input
    {
        self.filtered.clear();
        self.selected_index = 0;
        self.scroll_to_selected = true;
        if !self.visible {
            return;
        }

        // `:` prefix → match ex triggers; otherwise match labels/page names.
        let ex_mode = self.mode == PaletteMode::Commands && self.input.starts_with(':');
        let pattern: &str = if ex_mode {
            &self.input[1..]
        } else {
            &self.input[..]
        };
        let mut scored: Vec<(i64, usize)> = Vec::new();

        match self.mode {
            PaletteMode::Pages => {
                for (i, page) in self.pages.iter().enumerate() {
                    let score = match self.score(pattern, page) {
                        Some(s) => s,
                        None => continue,
                    };
                    scored.push((score, i));
                }
            }
            PaletteMode::Commands => {
                for (i, item) in self.items.iter().enumerate() {
                    let score = if ex_mode {
                        if item.category != PaletteCategory::Ex {
                            continue;
                        }
                        item.keys
                            .iter()
                            .filter_map(|k| self.score(pattern, k.trim_start_matches(':')))
                            .max()
                    } else {
                        self.score(pattern, item.label)
                    };
                    match score {
                        Some(s) => scored.push((s, i)),
                        None => continue,
                    }
                }
            }
        }

        scored.sort_by_key(|b| std::cmp::Reverse(b.0)); // stable: ties keep registry order
        self.filtered = scored.into_iter().map(|(_, i)| i).collect();
    }

    /** Score one choice against the pattern; empty patterns match everything.
     *  @param pattern - Fuzzy pattern (may be empty)
     *  @param choice - Text being matched
     *  @returns Some(score) when the choice matches
     */
    fn score(&self, pattern: &str, choice: &str) -> Option<i64> // Single fuzzy match
    {
        if pattern.is_empty() {
            Some(0)
        } else {
            self.matcher.fuzzy_match(choice, pattern)
        }
    }

    /** Move the highlighted row, clamped to the filtered list.
     *  @param delta - Rows to move by (negative = up)
     */
    pub fn move_selection(&mut self, delta: i32) // Keyboard/wheel selection
    {
        if self.filtered.is_empty() {
            return;
        }
        let last = (self.filtered.len() - 1) as i64;
        let next = (self.selected_index as i64 + delta as i64).clamp(0, last);
        if next as usize != self.selected_index {
            self.selected_index = next as usize;
            self.scroll_to_selected = true;
        }
    }

    /** Run the currently selected row.
     *  @returns The action to dispatch, raw ex input, or `None`
     */
    pub fn execute(&mut self) -> PaletteOutcome // Activate the highlighted row
    {
        if self.filtered.is_empty() {
            // Backward compatibility: run a typed ex command verbatim.
            if self.mode == PaletteMode::Commands && self.input.starts_with(':') {
                let raw = self.input.clone();
                self.close();
                return PaletteOutcome::Raw(raw);
            }
            return PaletteOutcome::None;
        }

        let row = self.selected_index.min(self.filtered.len() - 1);
        let idx = self.filtered[row];

        match self.mode {
            PaletteMode::Pages => {
                let name = self.pages[idx].clone();
                self.close();
                PaletteOutcome::Action(CommandAction::Open(name))
            }
            PaletteMode::Commands => {
                let item_id = self.items[idx].id;
                // `open_note` / `:e` switches to the page finder instead of closing.
                if item_id == "open_note" {
                    self.open_pages();
                    return PaletteOutcome::None;
                }
                let action = if item_id == "zen_mode" {
                    CommandAction::SetZenMode(!self.zen)
                } else {
                    self.items[idx].action.clone()
                };
                self.close();
                PaletteOutcome::Action(action)
            }
        }
    }

    /** Drain keyboard and wheel events while visible so they never reach vim.
     *  Pointer events are passed through untouched.
     *  @param i - Mutable egui input state for this frame
     *  @returns Action / raw-ex outcome, or `None`
     */
    pub fn handle_input(&mut self, i: &mut egui::InputState) -> PaletteOutcome // Keyboard routing
    {
        if !self.visible {
            return PaletteOutcome::None;
        }

        let mods = i.modifiers;
        let pointer_pos = i.pointer.latest_pos();
        let events = std::mem::take(&mut i.events);
        let mut keep = Vec::with_capacity(events.len());
        let mut outcome = PaletteOutcome::None;

        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    ..
                } => {
                    if !pressed || repeat {
                        continue;
                    }
                    match key {
                        egui::Key::Escape => {
                            self.close();
                        }
                        egui::Key::Enter => outcome = self.execute(),
                        egui::Key::ArrowDown => self.move_selection(1),
                        egui::Key::ArrowUp => self.move_selection(-1),
                        egui::Key::Tab => self.move_selection(if mods.shift { -1 } else { 1 }),
                        egui::Key::PageDown => self.move_selection(10),
                        egui::Key::PageUp => self.move_selection(-10),
                        egui::Key::Backspace => {
                            self.input.pop();
                            if self.input.is_empty() {
                                self.close();
                            } else {
                                self.recompute();
                            }
                        }
                        // Any other key is swallowed while the palette is open.
                        _ => {}
                    }
                }
                egui::Event::Text(s) => {
                    if !mods.ctrl && !mods.command && !mods.alt && !s.is_empty() {
                        self.input.push_str(&s);
                        self.recompute();
                    }
                }
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } => {
                    if pointer_pos.is_some_and(|p| self.rect.contains(p)) {
                        // Over the palette: keep the event for its ScrollArea.
                        keep.push(egui::Event::MouseWheel {
                            unit,
                            delta,
                            modifiers,
                            phase: egui::TouchPhase::Move,
                        });
                    } else {
                        // Not over the palette: drop it so the editor behind
                        // does not scroll while the overlay is open.
                        let lines = match unit {
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Point => delta.y / 20.0,
                            egui::MouseWheelUnit::Page => delta.y * 10.0,
                        };
                        if lines.abs() >= 1.0 {
                            self.move_selection(if lines < 0.0 { 1 } else { -1 });
                        }
                    }
                }
                other => keep.push(other),
            }
        }

        // Restore the events that are not palette input (pointer, focus, …).
        i.events = keep;
        outcome
    }

    /** Render the floating overlay above every panel.
     *  @param ctx - egui context (the palette lives on its own layer)
     *  @param theme - Palette colors come from `theme.palette`
     *  @returns Click outcome, or `None`
     */
    pub fn ui(&mut self, ctx: &egui::Context, theme: &Theme) -> Option<PaletteOutcome> // Overlay render
    {
        if !self.visible {
            return None;
        }

        let screen = ctx.content_rect();
        let width = (screen.width() * 0.6).clamp(380.0, 600.0);
        let y = screen.top() + screen.height() * 0.3;
        let mut outcome = None;

        let response = egui::Window::new("command_palette")
            .id(egui::Id::new("command_palette"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .min_width(width)
            .max_width(width)
            .default_width(width)
            .fixed_pos(egui::pos2(screen.center().x - width / 2.0, y))
            .frame(
                egui::Frame::new()
                    .fill(theme.palette.background)
                    .stroke(egui::Stroke::new(1.0, theme.palette.border))
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                self.render_input(ui, theme);
                self.render_list(ui, theme, &mut outcome);
                self.render_status(ui, theme);
            });

        if let Some(resp) = response {
            self.rect = resp.response.rect;
        }
        outcome
    }

    // ── Rendering ──

    /// Paint the input row with a trailing caret (or a placeholder hint).
    fn render_input(&self, ui: &mut egui::Ui, theme: &Theme) // Input line
    {
        let p = &theme.palette;
        let hint = if self.input.is_empty() {
            match self.mode {
                PaletteMode::Pages => "Type a page name…",
                PaletteMode::Commands => "Type a command…",
            }
        } else if self.input == ":" {
            "Type an ex command (:w, :q, :e …)"
        } else {
            ""
        };

        let (text, color) = if hint.is_empty() {
            (format!("{}_", self.input), p.foreground)
        } else {
            // Dimmed foreground reads better than a fill color as text.
            (hint.to_string(), theme.editor.line_numbers)
        };

        let galley = ui
            .painter()
            .layout_no_wrap(text, egui::FontId::monospace(14.0), color);
        let pos = ui.cursor().left_center();
        ui.painter().galley(
            egui::pos2(pos.x, pos.y - galley.size().y / 2.0),
            galley,
            color,
        );
        ui.add_space(22.0);
        self.paint_rule(ui, theme);
    }

    /// Paint the filtered rows: label, ex keys, keybind hint, tooltip.
    fn render_list(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        outcome: &mut Option<PaletteOutcome>,
    ) // Row list
    {
        let p = &theme.palette;
        let mut clicked: Option<usize> = None;

        egui::ScrollArea::vertical()
            .id_salt("command_palette_list")
            .max_height(320.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (row, &idx) in self.filtered.iter().enumerate() {
                    let selected = row == self.selected_index;
                    let (label, keys, keybind, tooltip) = match self.mode {
                        PaletteMode::Commands => {
                            let item = &self.items[idx];
                            (
                                item.label.to_string(),
                                item.keys.join(", "),
                                item.keybind.unwrap_or("").to_string(),
                                item.description.to_string(),
                            )
                        }
                        PaletteMode::Pages => (
                            self.pages[idx].clone(),
                            String::new(),
                            String::new(),
                            "Open page".to_string(),
                        ),
                    };

                    let resp = ui.allocate_response(
                        egui::vec2(ui.available_width(), 24.0),
                        egui::Sense::click(),
                    );
                    let row_rect = resp.rect;
                    let hovered = resp.hovered();
                    let clicked_row = resp.clicked();
                    if selected {
                        ui.painter().rect_filled(row_rect, 0.0, p.selected_bg);
                    } else if hovered {
                        ui.painter().rect_filled(row_rect, 0.0, p.input_bg);
                    }

                    let fg = if selected {
                        p.selected_fg
                    } else {
                        p.foreground
                    };
                    let dim = theme.editor.line_numbers;
                    let mid_y = row_rect.center().y;
                    let font = egui::FontId::monospace(13.0);

                    let label_galley = ui.painter().layout_no_wrap(label, font.clone(), fg);
                    let label_size = label_galley.size();
                    ui.painter().galley(
                        egui::pos2(row_rect.left() + 8.0, mid_y - label_size.y / 2.0),
                        label_galley,
                        fg,
                    );

                    if !keys.is_empty() {
                        ui.painter().text(
                            egui::pos2(row_rect.left() + 18.0 + label_size.x, mid_y),
                            egui::Align2::LEFT_CENTER,
                            keys,
                            font.clone(),
                            dim,
                        );
                    }
                    if !keybind.is_empty() {
                        ui.painter().text(
                            egui::pos2(row_rect.right() - 8.0, mid_y),
                            egui::Align2::RIGHT_CENTER,
                            keybind,
                            font,
                            dim,
                        );
                    }

                    if clicked_row {
                        clicked = Some(row);
                    }

                    // Keep keyboard selection in view without fighting the wheel.
                    if selected && self.scroll_to_selected {
                        ui.scroll_to_rect(row_rect, None);
                    }

                    // Consumes `resp`, so it must run last.
                    if hovered && !tooltip.is_empty() {
                        resp.on_hover_text(tooltip);
                    }
                }
                if self.scroll_to_selected {
                    self.scroll_to_selected = false;
                }
            });

        if let Some(row) = clicked {
            self.selected_index = row.min(self.filtered.len().saturating_sub(1));
            *outcome = Some(self.execute());
        }

        if self.filtered.is_empty() {
            self.paint_rule(ui, theme);
            ui.label(
                egui::RichText::new("No matches")
                    .monospace()
                    .color(theme.editor.line_numbers),
            );
        }
    }

    /// Paint the `N of M matches` footer.
    fn render_status(&self, ui: &mut egui::Ui, theme: &Theme) // Status hint line
    {
        if self.filtered.is_empty() {
            return;
        }
        self.paint_rule(ui, theme);
        ui.label(
            egui::RichText::new(format!(
                "{} of {} matches",
                self.filtered.len(),
                self.total()
            ))
            .monospace()
            .color(theme.editor.line_numbers),
        );
    }

    /// Paint a thin themed separator line at the current cursor.
    fn paint_rule(&self, ui: &mut egui::Ui, theme: &Theme) // Themed horizontal rule
    {
        let y = ui.cursor().top() + 4.0;
        let rect = ui.max_rect();
        ui.painter().hline(
            rect.left()..=rect.right(),
            y,
            egui::Stroke::new(1.0, theme.palette.border),
        );
        ui.add_space(9.0);
    }

    /// Unfiltered row count for the current mode.
    fn total(&self) -> usize // Registry/page count
    {
        match self.mode {
            PaletteMode::Commands => self.items.len(),
            PaletteMode::Pages => self.pages.len(),
        }
    }
}

/// The complete Phase 4 command registry (ROADMAP.md palette table).
fn registry() -> Vec<PaletteItem> // Static command list
{
    use PaletteCategory::{App, Ex};
    vec![
        PaletteItem {
            id: "save",
            label: "Save",
            keys: &[":w", ":write"],
            description: "Write buffer to disk",
            keybind: Some("Ctrl+S"),
            category: Ex,
            action: CommandAction::Save,
        },
        PaletteItem {
            id: "quit",
            label: "Quit",
            keys: &[":q", ":quit"],
            description: "Quit Wid",
            keybind: None,
            category: Ex,
            action: CommandAction::Quit,
        },
        PaletteItem {
            id: "save_quit",
            label: "Save & Quit",
            keys: &[":wq", ":x"],
            description: "Save and quit",
            keybind: None,
            category: Ex,
            action: CommandAction::SaveQuit,
        },
        PaletteItem {
            id: "force_quit",
            label: "Force Quit",
            keys: &[":q!", ":quit!"],
            description: "Quit without saving",
            keybind: None,
            category: Ex,
            action: CommandAction::ForceQuit,
        },
        PaletteItem {
            id: "force_save_quit",
            label: "Force Save & Quit",
            keys: &[":wq!"],
            description: "Force save and quit",
            keybind: None,
            category: Ex,
            action: CommandAction::SaveForceQuit,
        },
        PaletteItem {
            id: "toggle_preview",
            label: "Toggle Preview",
            keys: &[],
            description: "Show/hide markdown preview",
            keybind: None,
            category: App,
            action: CommandAction::TogglePreview,
        },
        PaletteItem {
            id: "toggle_sidebar",
            label: "Toggle Sidebar",
            keys: &[],
            description: "Show/hide sidebar",
            keybind: None,
            category: App,
            action: CommandAction::ToggleSidebar,
        },
        PaletteItem {
            id: "create_page",
            label: "Create Note",
            keys: &[],
            description: "Create a new page",
            keybind: None,
            category: App,
            action: CommandAction::CreatePage,
        },
        PaletteItem {
            id: "delete_page",
            label: "Delete Page",
            keys: &[],
            description: "Delete current page",
            keybind: None,
            category: App,
            action: CommandAction::DeletePage,
        },
        PaletteItem {
            id: "open_note",
            label: "Open Note",
            keys: &[":e", ":open"],
            description: "Fuzzy-find and switch pages",
            keybind: Some("Ctrl+P"),
            category: Ex,
            action: CommandAction::OpenNote,
        },
        PaletteItem {
            id: "search_tags",
            label: "Search Tags",
            keys: &[],
            description: "Search by #tag",
            keybind: None,
            category: App,
            action: CommandAction::SearchTags,
        },
        PaletteItem {
            id: "switch_theme",
            label: "Switch Theme",
            keys: &[],
            description: "Cycle to the next built-in theme",
            keybind: None,
            category: App,
            action: CommandAction::SwitchTheme(String::new()),
        },
        PaletteItem {
            id: "open_workspace",
            label: "Open Workspace",
            keys: &[],
            description: "Open a different workspace",
            keybind: None,
            category: App,
            action: CommandAction::OpenWorkspace,
        },
        PaletteItem {
            id: "today_journal",
            label: "Today's Journal",
            keys: &[],
            description: "Jump to today's journal",
            keybind: None,
            category: App,
            action: CommandAction::TodayJournal,
        },
        PaletteItem {
            id: "help",
            label: "Help",
            keys: &[":help"],
            description: "Display help buffer",
            keybind: None,
            category: Ex,
            action: CommandAction::Ex(":help".to_string()),
        },
        PaletteItem {
            id: "clear_search",
            label: "Clear Search",
            keys: &[":noh", ":nohlsearch"],
            description: "Clear search highlights",
            keybind: None,
            category: Ex,
            action: CommandAction::Ex(":noh".to_string()),
        },
        PaletteItem {
            id: "zen_mode",
            label: "Zen Mode",
            keys: &[],
            description: "Fullscreen editor only",
            keybind: Some("F11"),
            category: App,
            action: CommandAction::SetZenMode(true),
        },
        PaletteItem {
            id: "toggle_menu_bar",
            label: "Toggle Menu Bar",
            keys: &[":menubar"],
            description: "Show/hide the menu bar",
            keybind: Some("Alt"),
            category: Ex,
            action: CommandAction::ToggleMenuBar,
        },
    ]
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;

    /// Type one character into the palette and re-run the filter.
    fn type_char(p: &mut CommandPalette, c: char) // Simulate a keystroke
    {
        p.input.push(c);
        p.recompute();
    }

    /// A `:`-prefixed input only ever matches Ex-category registry rows.
    #[test]
    fn ex_prefix_filters_to_ex_commands_only() // Roadmap: `:` → ex list
    {
        let mut p = CommandPalette::new();
        p.open_commands(":");
        type_char(&mut p, 'w');

        assert!(!p.filtered.is_empty(), "`:w` must match something");
        for &idx in &p.filtered {
            assert_eq!(p.items[idx].category, PaletteCategory::Ex);
            assert!(
                p.items[idx].keys.iter().any(|k| k.contains('w')),
                "row {} should contain `w`",
                p.items[idx].id
            );
        }
        // `:w` itself must outrank longer variants like `:wq!`.
        let top = p.items[p.filtered[0]].id;
        assert_eq!(top, "save");
    }

    /// Without a `:` prefix, matching runs over display labels instead.
    #[test]
    fn command_mode_matches_labels() // Roadmap: Ctrl+Shift+P list
    {
        let mut p = CommandPalette::new();
        p.open_commands("");
        assert_eq!(p.filtered.len(), p.items.len(), "empty input shows all");

        type_char(&mut p, 's');
        type_char(&mut p, 'a');
        type_char(&mut p, 'v');
        assert!(!p.filtered.is_empty());
        let top = &p.items[p.filtered[0]];
        assert_eq!(top.id, "save");
        assert_eq!(top.label, "Save");
    }

    /// Selection movement is clamped at both ends of the filtered list.
    #[test]
    fn selection_clamps_to_bounds() // Keyboard navigation
    {
        let mut p = CommandPalette::new();
        p.open_commands("");
        let last = p.filtered.len() - 1;

        p.move_selection(-5);
        assert_eq!(p.selected_index, 0);
        p.move_selection(1_000);
        assert_eq!(p.selected_index, last);
        p.move_selection(-1);
        assert_eq!(p.selected_index, last - 1);
    }

    /// An unmatched ex input falls through to `VimState::execute_command`.
    #[test]
    fn unmatched_ex_input_executes_verbatim() // Roadmap: backward compat
    {
        let mut p = CommandPalette::new();
        p.open_commands(":");
        for c in "sneaky".chars() {
            type_char(&mut p, c);
        }
        assert!(p.filtered.is_empty());

        match p.execute() {
            PaletteOutcome::Raw(cmd) => assert_eq!(cmd, ":sneaky"),
            _ => panic!("unmatched ex input must produce Raw(:sneaky)"),
        }
        assert!(!p.visible, "execute() must dismiss the palette");
    }

    /// Selecting a registered row returns its action and closes the overlay.
    #[test]
    fn execute_dispatches_selected_action() // Enter → CommandAction
    {
        let mut p = CommandPalette::new();
        p.open_commands("");
        type_char(&mut p, 's');
        type_char(&mut p, 'a');
        type_char(&mut p, 'v');

        match p.execute() {
            PaletteOutcome::Action(CommandAction::Save) => {}
            _ => panic!("expected Save as the top match"),
        }
        assert!(!p.visible);
    }

    /// The `open_note` row switches to the page finder instead of closing.
    #[test]
    fn open_note_switches_to_page_finder() // `:e` → Ctrl+P mode
    {
        let mut p = CommandPalette::new();
        p.open_commands(":");
        type_char(&mut p, 'e');

        let row = p
            .filtered
            .iter()
            .position(|&i| p.items[i].id == "open_note")
            .expect("`:e` must match open_note");
        p.selected_index = row;

        let outcome = p.execute();
        assert!(matches!(outcome, PaletteOutcome::None));
        assert!(p.visible, "page finder should stay open");
        assert_eq!(p.mode, PaletteMode::Pages);
    }

    /// Pages mode fuzzy-finds page names and opens the selected one.
    #[test]
    fn page_finder_opens_matched_page() // Ctrl+P flow
    {
        let mut p = CommandPalette::new();
        p.pages = vec!["Welcome".into(), "Theme-Reference".into()];
        p.open_pages();
        assert_eq!(p.filtered.len(), 2);

        for c in "theme".chars() {
            type_char(&mut p, c);
        }
        assert_eq!(p.filtered.len(), 1);
        match p.execute() {
            PaletteOutcome::Action(CommandAction::Open(name)) => {
                assert_eq!(name, "Theme-Reference");
            }
            _ => panic!("page selection must produce Open(Theme-Reference)"),
        }
    }

    /// The zen row flips based on the mirrored state so it can turn off too.
    #[test]
    fn zen_item_toggles_both_ways() // SetZenMode mirror
    {
        let mut p = CommandPalette::new();
        p.open_commands("");
        let row = p
            .filtered
            .iter()
            .position(|&i| p.items[i].id == "zen_mode")
            .unwrap();
        p.selected_index = row;

        p.zen = false;
        assert!(matches!(
            p.execute(),
            PaletteOutcome::Action(CommandAction::SetZenMode(true))
        ));

        p.open_commands("");
        p.selected_index = p
            .filtered
            .iter()
            .position(|&i| p.items[i].id == "zen_mode")
            .unwrap();
        p.zen = true;
        assert!(matches!(
            p.execute(),
            PaletteOutcome::Action(CommandAction::SetZenMode(false))
        ));
    }

    /// The registry covers every command in the ROADMAP Phase 4 table.
    #[test]
    fn registry_covers_roadmap_table() // 18 entries
    {
        let items = registry();
        assert_eq!(items.len(), 18);
        let mut ids: Vec<&str> = items.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 18, "registry ids must be unique");
    }
}
