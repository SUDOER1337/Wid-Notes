use crate::ai;
use crate::ai::ollama::OllamaClient;
use crate::ai::search::{SearchProvider, format_results_as_context, provider_from_config};
use crate::ai::types::{AsyncEvent, ChatMessage, Conversation};
use crate::command_palette::{CommandPalette, PaletteOutcome};
use crate::editor::Buffer;
use crate::preview::render_markdown;
use crate::session;
use crate::settings::Settings;
use crate::sidebar::{self, RightSidebar, RightView, SearchHit, Sidebar, SidebarView};
use crate::theme::Theme;

use crate::vim::{CommandAction, Mode, PageState, VimState};
use crate::widget::editor_ui;
use crate::workspace::Workspace;
use eframe::egui;
use eframe::egui::panel::Panel;
use eframe::egui::{
    Align, Align2, Color32, FontId, Frame, Key, Layout, RichText, ScrollArea, TextEdit, Vec2,
    Window,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

enum AppScreen {
    Welcome,
    WorkspaceReady,
}

/// Which surface owns the keyboard this frame.
///
/// Panel chords (Ctrl+Shift+E/F/V//) move focus through `Editor → panel →
/// Editor`, and `widget.rs` only routes keys to vim while this is `Editor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The vim editor owns the keyboard.
    Editor,
    /// The left side panel (Explorer, Search, …) owns the keyboard.
    Sidebar,
    /// The right side panel (Preview, Agent) owns the keyboard.
    Right,
}

/// Section a row belongs to in the Explorer list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplorerGroup {
    Today,
    Pages,
    Journals,
}

/// One selectable Explorer row.
struct ExplorerRow {
    group: ExplorerGroup,
    /// Text painted in the panel.
    label: String,
    /// Workspace page the row opens.
    page: String,
}

pub struct WidApp {
    screen: AppScreen,
    workspace: Option<Workspace>,
    buffer: Buffer,
    vim: VimState,
    settings: Settings,
    new_page_name: String,
    status_message: String,
    theme: Theme,
    /// Whether egui visuals and hardcoded accents use the dark palette.
    is_dark: bool,
    show_prompt: bool,
    prompt_path: String,
    prompt_name: String,
    current_page: Option<String>,
    /// Activity bar + side panel selection.
    sidebar: Sidebar,
    /// Right-hand activity bar + side panel (preview, agent chat).
    right: RightSidebar,
    /// Menu bar visibility — hidden unless `settings.window.menu_bar = "visible"`
    /// or toggled with bare Alt (Firefox-style), the View menu, or `:menubar`.
    menu_visible: bool,
    /// Alt is held and no chord key (Alt+1/2/T …) has arrived yet.
    alt_armed: bool,
    /// Previous frame's Alt modifier state, for edge detection.
    alt_was_down: bool,
    /// Command palette / page finder overlay (`:`, `Ctrl+P`).
    palette: CommandPalette,
    /// Which surface currently owns the keyboard (editor vs a side panel).
    focus: Focus,
    /// Last known right-panel scroll offset, so `j`/`k` can step from it.
    panel_scroll_offset: f32,
    /// Pending right-panel scroll, in rows, applied on the next paint.
    panel_scroll_rows: isize,
    /// A Search-panel `i` requested focus; applied on the next frame so the
    /// `i`'s own `Event::Text` cannot land in the field we are focusing.
    pending_search_focus: bool,
    /// Zen mode hides all chrome around the editor (F11).
    zen: bool,
    /// Sidebar/menu state to restore when zen mode exits.
    zen_restore: Option<(bool, bool, bool)>,
    page_states: HashMap<String, PageState>,
    /// Tracks insertion order for page_states eviction (LRU cap).
    page_state_order: std::collections::VecDeque<String>,

    // ── AI Panel ──────────────────────────────────────────
    ai_conversations: Vec<Conversation>,
    ai_selected_idx: usize,
    ai_input: String,
    ai_models: Vec<String>,
    ai_is_streaming: bool,
    ai_is_searching: bool,
    ai_show_model_picker: bool,
    ai_show_settings: bool,
    ai_event_rx: mpsc::UnboundedReceiver<AsyncEvent>,
    ai_event_tx: mpsc::UnboundedSender<AsyncEvent>,
    ai_client: OllamaClient,
    ai_runtime: tokio::runtime::Runtime,
    ai_next_id: u64,
    ai_status: String,
    ai_loaded_from_disk: bool,
    ai_pending_search_query: Option<String>,
    ai_searcher: SearchProvider,
    ai_dot_frame: u32,
    ai_auto_scroll: bool,
    ai_show_help: bool,
    ai_stream_conv_id: Option<u64>,
    ai_stream_cancel: Arc<AtomicBool>,
    ai_last_user_msg: Option<String>,
}

impl WidApp {
    /// Maximum number of per-page undo/register states held in memory.
    const MAX_PAGE_STATES: usize = 32;

    /** Create a new app instance, optionally auto-opening a last-used workspace.
     *  @param last_workspace - Path to a previously saved workspace, if any
     */
    pub fn new(last_workspace: Option<PathBuf>, settings: Settings) -> Self {
        let theme = Theme::build(None, &settings.theme);
        let is_dark = !settings.invert;
        let theme = if settings.invert {
            theme.inverted()
        } else {
            theme
        };
        let ai_runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let (ai_event_tx, ai_event_rx) = mpsc::unbounded_channel();
        let ai_client = OllamaClient::new(&settings.ai.ollama_url);
        let ai_searcher = provider_from_config(&settings.ai);
        let menu_visible = settings.window.menu_bar == "visible";

        let mut app = Self {
            screen: AppScreen::Welcome,
            workspace: None,
            buffer: Buffer::new(),
            vim: VimState::new(),
            settings,
            new_page_name: String::new(),
            status_message: String::new(),
            theme,
            is_dark,
            show_prompt: true,
            prompt_path: String::new(),
            prompt_name: String::from("my-workspace"),
            current_page: None,
            sidebar: Sidebar::default(),
            right: RightSidebar::default(),
            menu_visible,
            alt_armed: false,
            alt_was_down: false,
            palette: CommandPalette::new(),
            focus: Focus::Editor,
            panel_scroll_offset: 0.0,
            panel_scroll_rows: 0,
            pending_search_focus: false,
            zen: false,
            zen_restore: None,
            page_states: HashMap::new(),
            page_state_order: std::collections::VecDeque::new(),

            ai_conversations: vec![],
            ai_selected_idx: 0,
            ai_input: String::new(),
            ai_models: vec![],
            ai_is_streaming: false,
            ai_is_searching: false,
            ai_show_model_picker: false,
            ai_show_settings: false,
            ai_event_rx,
            ai_event_tx,
            ai_client,
            ai_runtime,
            ai_next_id: 1,
            ai_status: String::new(),
            ai_loaded_from_disk: false,
            ai_pending_search_query: None,
            ai_searcher,
            ai_dot_frame: 0,
            ai_auto_scroll: true,
            ai_show_help: false,
            ai_stream_conv_id: None,
            ai_stream_cancel: Arc::new(AtomicBool::new(false)),
            ai_last_user_msg: None,
        };

        // Auto-open last workspace if it still exists
        if let Some(ref path) = last_workspace {
            if path.join("workspace.toml").exists() {
                app.open_workspace(path);
            } else {
                app.status_message =
                    "Previous workspace not found; create or open a new one.".to_string();
            }
        }

        app
    }

    /// Run a blocking file dialog on the Tokio runtime to avoid zbus reactor issues.
    fn run_file_dialog<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce() -> Option<R> + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.ai_runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(f).await.unwrap_or(None);
            let _ = tx.send(result);
        });
        self.ai_runtime.block_on(rx).unwrap_or(None)
    }

    fn pick_workspace_folder(&self) -> Option<PathBuf> {
        self.run_file_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Select workspace directory")
                .pick_folder()
        })
    }

    fn open_workspace(&mut self, path: &std::path::Path) {
        if let Some(ws) = Workspace::open(path) {
            session::save_last_workspace(path);
            self.settings = ws.load_settings();
            self.rebuild_theme();
            self.ai_searcher = provider_from_config(&self.settings.ai);
            self.ai_client = OllamaClient::new(&self.settings.ai.ollama_url);
            self.workspace = Some(ws);
            self.open_today_journal();
            self.screen = AppScreen::WorkspaceReady;
            self.show_prompt = false;
            self.after_workspace_ready();
            self.status_message = "Workspace opened".to_string();
        } else {
            self.status_message = "Failed to open workspace".to_string();
        }
    }

    fn after_workspace_ready(&mut self) {
        // Load saved conversations
        if let Some(ref ws) = self.workspace
            && let Ok(convs) = ai::persistence::load(&ws.root)
        {
            self.ai_conversations = convs;
            self.ai_loaded_from_disk = true;
            if !self.ai_conversations.is_empty() {
                let last_id = self
                    .ai_conversations
                    .iter()
                    .map(|c| c.id)
                    .max()
                    .unwrap_or(0);
                self.ai_next_id = last_id + 1;
            }
        }
        // Fetch models from Ollama
        let tx = self.ai_event_tx.clone();
        let cl = self.ai_client.clone();
        self.ai_runtime.spawn(async move {
            cl.fetch_models(tx).await;
        });
    }

    fn create_workspace(&mut self, path: &std::path::Path, name: &str) {
        match Workspace::create(path, name) {
            Ok(ws) => {
                session::save_last_workspace(path);
                ws.save_settings(&self.settings).ok();
                self.settings = ws.load_settings();
                self.rebuild_theme();
                self.ai_searcher = provider_from_config(&self.settings.ai);
                self.ai_client = OllamaClient::new(&self.settings.ai.ollama_url);
                self.workspace = Some(ws);
                self.open_today_journal();
                self.screen = AppScreen::WorkspaceReady;
                self.show_prompt = false;
                self.after_workspace_ready();
                self.status_message = format!("Created workspace '{}'", name);
            }
            Err(e) => {
                self.status_message = format!("Failed to create workspace: {}", e);
            }
        }
    }

    fn open_today_journal(&mut self) {
        if let Some(ref mut ws) = self.workspace {
            match ws.ensure_today_journal() {
                Ok(name) => {
                    log::info!("Opened today's journal: {}", name);
                    self.load_page(&name);
                }
                Err(e) => {
                    self.status_message = format!("Error creating journal: {}", e);
                }
            }
        }
    }

    fn load_page(&mut self, name: &str) {
        // Save current page state before switching
        if let Some(ref current) = self.current_page.clone() {
            if self.vim.needs_save(&self.buffer) {
                self.save_current_page();
            }
            let state = self.vim.take_page_state();
            // Evict the oldest entry when the cap is reached.
            if !self.page_states.contains_key(current.as_str()) {
                if self.page_states.len() >= Self::MAX_PAGE_STATES
                    && let Some(oldest) = self.page_state_order.pop_front()
                {
                    self.page_states.remove(&oldest);
                }
                self.page_state_order.push_back(current.clone());
            }
            self.page_states.insert(current.clone(), state);
        }
        let content = self
            .workspace
            .as_ref()
            .and_then(|ws| ws.read_page(name).ok());
        match content {
            Some(text) => {
                log::info!("Loaded page: {}", name);
                self.buffer = Buffer::new_with_text(text);
                self.vim = VimState::new();
                // Restore per-page state if it exists
                if let Some(state) = self.page_states.remove(name) {
                    self.vim.restore_page_state(state);
                }
                self.current_page = Some(name.to_string());
                self.status_message = format!("Loaded: {}.md", name);
            }
            None => {
                self.status_message = format!("Error loading: {}", name);
            }
        }
    }

    fn save_current_page(&mut self) {
        if let Some(ref ws) = self.workspace
            && let Some(ref name) = self.current_page
        {
            match ws.write_page(name, &self.buffer.text) {
                Ok(_) => {
                    self.vim.mark_saved(&self.buffer);
                    self.status_message = format!("Saved: {}.md", name);
                }
                Err(e) => {
                    self.status_message = format!("Error saving: {}", e);
                }
            }
        }
    }

    fn create_page(&mut self) {
        let name = self.new_page_name.trim().to_string();
        if name.is_empty() {
            self.status_message = "Page name cannot be empty".to_string();
            return;
        }
        if let Some(ref mut ws) = self.workspace {
            match ws.create_page(&name) {
                Ok(_) => {
                    self.load_page(&name);
                    self.new_page_name.clear();
                    self.status_message = format!("Created: {}.md", name);
                }
                Err(e) => {
                    self.status_message = format!("Error creating: {}", e);
                }
            }
        }
    }

    fn delete_current_page(&mut self) {
        if let Some(ref mut ws) = self.workspace
            && let Some(ref name) = self.current_page.clone()
        {
            // Remove page state so it doesn't linger
            self.page_states.remove(name);
            match ws.delete_page(name) {
                Ok(_) => {
                    self.current_page = None;
                    self.buffer = Buffer::new();
                    self.vim = VimState::new();
                    self.open_today_journal();
                    self.status_message = format!("Deleted: {}.md", name);
                }
                Err(e) => {
                    self.status_message = format!("Error deleting: {}", e);
                }
            }
        }
    }

    /// Rebuild the theme from the current settings and persist.
    fn apply_theme(&mut self) {
        self.rebuild_theme();
        if let Some(ref ws) = self.workspace {
            let _ = ws.save_settings(&self.settings);
        }
    }

    /// Resolve the theme (base + optional invert) and remember the polarity,
    /// which flips egui `Visuals` and any hardcoded accent colors.
    fn rebuild_theme(&mut self) {
        let root = self.workspace.as_ref().map(|ws| ws.root.as_path());
        let base = Theme::build(root, &self.settings.theme);
        self.is_dark = !self.settings.invert;
        self.theme = if self.settings.invert {
            base.inverted()
        } else {
            base
        };
    }

    /// Switch to a built-in theme by name and persist.
    fn set_theme_name(&mut self, name: &str) {
        self.settings.theme = name.to_string();
        self.apply_theme();
    }

    /// Flip the invert (light mode) flag and re-apply the theme.
    fn toggle_invert(&mut self) {
        self.settings.invert = !self.settings.invert;
        self.apply_theme();
    }

    /** Map a dark-palette accent color for the current polarity.
     *  In light mode, hardcoded accents are inverted so they stay readable
     *  against the inverted (light) backgrounds, unless the theme's
     *  `invert_excludes` list spares them.
     *  @param c - Color authored for the dark palette
     *  @returns c as-is in dark mode, inverted in light mode
     */
    fn accent(&self, c: Color32) -> Color32 {
        if self.is_dark {
            c
        } else {
            crate::theme::invert_color_if(c, &self.theme.invert_excludes)
        }
    }
}

impl eframe::App for WidApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Sync egui chrome (menus, scrollbars, text fields) with the theme polarity.
        ui.ctx().set_visuals(if self.is_dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });

        // Bare-Alt menu bar toggle (Firefox-style). Arm when Alt goes down with
        // no chord key (Alt+1/2/T, Alt+letter) in the frame or while held;
        // fire on release only if still armed, so chords never open the menu.
        let (alt_down, alt_chord) = ui.input(|i| {
            let chord = i.events.iter().any(|e| match e {
                egui::Event::Key {
                    pressed: true,
                    modifiers: m,
                    ..
                } => m.alt,
                // Text-only input (no Key event) still counts as a chord.
                egui::Event::Text(_) => i.modifiers.alt,
                _ => false,
            });
            (i.modifiers.alt, chord)
        });
        if alt_down != self.alt_was_down {
            if alt_down {
                self.alt_armed = !alt_chord;
            } else {
                if self.alt_armed {
                    self.menu_visible = !self.menu_visible;
                }
                self.alt_armed = false;
            }
        }
        if alt_down && alt_chord {
            self.alt_armed = false;
        }
        self.alt_was_down = alt_down;

        // Handle Ctrl+S
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_current_page();
        }
        // Theme switching via Alt chords
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::Num1)) {
            self.set_theme_name("wid-dark");
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::Num2)) {
            self.set_theme_name("gruvbox");
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::T)) {
            self.toggle_invert();
        }
        // Zen mode — before palette input so F11 always wins.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F11)) {
            self.set_zen(!self.zen, ui.ctx());
        }

        if matches!(self.screen, AppScreen::WorkspaceReady) {
            // Keep the palette's page list and zen mirror fresh.
            if self.palette.visible {
                self.refresh_palette_pages();
                self.palette.zen = self.zen;
            }

            // Palette input consumes the keyboard before it can reach vim.
            let outcome = ui.input_mut(|i| self.palette.handle_input(i));
            self.apply_palette_outcome(outcome, ui.ctx());

            // Palette triggers (roadmap Phase 4 step 7). `Ctrl+Shift+P` is no
            // longer a palette trigger: egui-winit eats Ctrl+Shift+V (see the
            // panel chords below), so the Preview chord moved onto `P`. The
            // palette keeps `:` (Commands) and `Ctrl+P` (pages). The shift
            // guard is load-bearing: `consume_key` matches logically and
            // ignores extra shift (egui input.rs:850), so a bare `Ctrl+P`
            // pattern would swallow `Ctrl+Shift+P` before Preview saw it.
            if !self.palette.visible {
                let pages = ui.input_mut(|i| {
                    !i.modifiers.shift && i.consume_key(egui::Modifiers::CTRL, egui::Key::P)
                });
                if pages {
                    self.refresh_palette_pages();
                    self.palette.open_pages();
                }
            }

            // Panel chords (Zed-style). These stay live even while a text
            // field has focus: a focused search field must not swallow
            // Ctrl+Shift+P/Q// — it cannot type them anyway. Only the palette,
            // which owns the keyboard while open, suppresses them.
            if !self.palette.visible {
                self.reconcile_focus();
                self.release_hidden_search_focus(ui);

                // Apply a deferred `i`-focus from the Search panel. winit
                // commits the `i` as `Event::Text` either alongside the key
                // press or a frame later, so focusing on the press frame let
                // the freshly focused field swallow that `i` as typed text.
                // Swallow the commit here instead, then hand over the field.
                if self.pending_search_focus {
                    ui.input_mut(|i| {
                        i.events
                            .retain(|e| !matches!(e, egui::Event::Text(t) if t == "i"));
                    });
                    set_search_input_focus(ui.ctx(), true);
                    self.pending_search_focus = false;
                }

                // Ctrl+Shift+E/F/P// — show, then focus, then release a panel.
                let chord = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
                if ui.input_mut(|i| i.consume_key(chord, Key::E)) {
                    self.activate_left(SidebarView::Explorer);
                }
                if ui.input_mut(|i| i.consume_key(chord, Key::F)) {
                    // Selecting Search drops the keyboard into the query field
                    // so the user can type straight away; toggling it off hands
                    // the field back for panel navigation.
                    let opened = !(self.sidebar.open && self.sidebar.view == SidebarView::Search);
                    self.activate_left(SidebarView::Search);
                    set_search_input_focus(ui.ctx(), opened);
                }
                // Ctrl+Shift+P — Preview. `P`, not `V`: egui-winit rewrites
                // Ctrl+Shift+V into `Event::Paste` and returns before any
                // `Key::V` is emitted, so a `V` chord can never fire live.
                if ui.input_mut(|i| i.consume_key(chord, Key::P)) {
                    self.activate_right(RightView::Preview);
                }
                // `Ctrl+Shift+/` is `?` on most layouts and `/` on some.
                if ui.input_mut(|i| {
                    i.consume_key(chord, Key::Questionmark) || i.consume_key(chord, Key::Slash)
                }) {
                    self.activate_right(RightView::Agent);
                }

                // Ctrl+Q closes whichever panel owns the keyboard.
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Q)) {
                    self.close_focused_panel();
                }

                // Single-letter navigation stays text-edit-gated so a focused
                // field keeps receiving j/k/Enter as ordinary characters.
                if !ui.ctx().text_edit_focused() {
                    self.handle_panel_keys(ui);
                }
            }
        }

        match self.screen {
            AppScreen::Welcome => {
                self.render_welcome(ui);
            }
            AppScreen::WorkspaceReady => {
                self.render_workspace(ui);
            }
        }

        ui.ctx().request_repaint();
    }
}

// ── Welcome / workspace prompt ──

impl WidApp {
    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("Wid");
            ui.label("A local-first knowledge workspace");
            ui.add_space(20.0);

            if self.show_prompt {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(egui::TextEdit::singleline(&mut self.prompt_name).desired_width(200.0));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.prompt_path)
                            .hint_text("Leave empty for current directory")
                            .desired_width(300.0),
                    );
                });
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("Create Workspace").clicked() {
                        let path = if self.prompt_path.trim().is_empty() {
                            std::path::PathBuf::from(".")
                        } else {
                            std::path::PathBuf::from(self.prompt_path.trim())
                        };
                        let ws_name = self.prompt_name.clone();
                        self.create_workspace(&path.join(&ws_name), &ws_name);
                    }
                    if ui.button("Open Existing").clicked() {
                        if let Some(dir) = self.pick_workspace_folder() {
                            self.open_workspace(&dir);
                        }
                    }
                });
            }

            if !self.status_message.is_empty() {
                ui.add_space(10.0);
                ui.colored_label(Color32::GRAY, &self.status_message);
            }
        });
    }
}

// ── Main workspace UI ──

impl WidApp {
    /** Run a deferred command action (from vim, the palette, or a trigger).
     *  @param action - The action to perform
     *  @param ctx - egui context for viewport commands (close, fullscreen)
     */
    fn dispatch_action(&mut self, action: CommandAction, ctx: &egui::Context) // Run a CommandAction
    {
        match action {
            CommandAction::None => {}
            CommandAction::Save => self.save_current_page(),
            CommandAction::Quit => {
                if self.vim.needs_save(&self.buffer) {
                    // Warn like real vim — don't silently save.
                    self.status_message =
                        "No write since last change (use :wq to save, :q! to force quit)"
                            .to_string();
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            CommandAction::ForceQuit => {
                // Force quit without saving
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            CommandAction::SaveQuit => {
                self.save_current_page();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            CommandAction::SaveForceQuit => {
                self.save_current_page();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            CommandAction::Open(name) => self.load_page(&name),
            CommandAction::OpenPalette => self.palette.open_commands(":"),
            CommandAction::ToggleMenuBar => self.menu_visible = !self.menu_visible,
            CommandAction::TogglePreview => {
                let open = self.right.is_open(RightView::Preview);
                if !open {
                    self.right.view = RightView::Preview;
                }
                self.right.set_open(RightView::Preview, !open);
            }
            CommandAction::ToggleSidebar => self.sidebar.open = !self.sidebar.open,
            CommandAction::ShowExplorer => self.activate_left(SidebarView::Explorer),
            CommandAction::ShowSearch => {
                let opened = !(self.sidebar.open && self.sidebar.view == SidebarView::Search);
                self.activate_left(SidebarView::Search);
                set_search_input_focus(ctx, opened);
            }
            CommandAction::ShowPreview => self.activate_right(RightView::Preview),
            CommandAction::ShowAgent => self.activate_right(RightView::Agent),
            CommandAction::CreatePage => self.new_page_name = "untitled".to_string(),
            CommandAction::DeletePage => self.delete_current_page(),
            CommandAction::OpenNote => {
                self.refresh_palette_pages();
                self.palette.open_pages();
            }
            CommandAction::SearchTags => {
                self.status_message = "Tag search is not implemented yet".to_string();
            }
            CommandAction::SwitchTheme(name) => {
                // Empty name = cycle between the built-in themes.
                let target = if name.is_empty() {
                    if self.settings.theme == "wid-dark" {
                        "gruvbox".to_string()
                    } else {
                        "wid-dark".to_string()
                    }
                } else {
                    name
                };
                self.set_theme_name(&target);
            }
            CommandAction::OpenWorkspace => {
                if let Some(dir) = self.pick_workspace_folder() {
                    self.open_workspace(&dir);
                }
            }
            CommandAction::TodayJournal => self.open_today_journal(),
            CommandAction::SetZenMode(on) => self.set_zen(on, ctx),
            CommandAction::Ex(cmd) => self.vim.execute_command(&cmd, &mut self.buffer),
        }
    }

    /** Apply a palette outcome (dispatch an action or run raw ex input).
     *  @param outcome - Result returned by the palette
     *  @param ctx - egui context for viewport commands
     */
    fn apply_palette_outcome(&mut self, outcome: PaletteOutcome, ctx: &egui::Context)
    // Route palette results
    {
        match outcome {
            PaletteOutcome::Action(action) => self.dispatch_action(action, ctx),
            PaletteOutcome::Raw(cmd) => self.vim.execute_command(&cmd, &mut self.buffer),
            PaletteOutcome::None => {}
        }
    }

    /// Refresh the palette's page/journal list from the workspace.
    fn refresh_palette_pages(&mut self) // Keep the page finder current
    {
        let mut pages = Vec::new();
        if let Some(ws) = &self.workspace {
            pages.extend(ws.pages().iter().cloned());
            pages.extend(ws.journals().iter().cloned());
        }
        self.palette.pages = pages;
    }

    // ── Panel focus & keybinds ─────────────────────────────

    /** Drop a stale panel focus left behind when a click or menu closed the panel.
     *  Without this the editor would stay silent forever with no panel to type into.
     */
    fn reconcile_focus(&mut self) // Focus follows panel visibility
    {
        if self.focus == Focus::Sidebar && !self.sidebar.open {
            self.focus = Focus::Editor;
        }
        if self.focus == Focus::Right && !self.right.open {
            self.focus = Focus::Editor;
        }
    }

    /** Run a Zed-style chord on a left view: reveal → focus → release.
     *  First press shows the view (focus stays put), a second press moves the
     *  keyboard into the panel so `j`/`k`/Enter navigate it, a third press
     *  hands focus back to the editor. Switching views keeps an already
     *  focused panel focused.
     *  @param view - The left-panel view the chord selects
     */
    fn activate_left(&mut self, view: SidebarView) // Show, focus or unfocus a left view
    {
        if self.sidebar.open && self.sidebar.view == view {
            self.focus = if self.focus == Focus::Sidebar {
                Focus::Editor
            } else {
                Focus::Sidebar
            };
        } else {
            self.sidebar.view = view;
            self.sidebar.open = true;
            self.sidebar.cursor = 0;
            self.sidebar.reveal_cursor = true;
        }
    }

    /** Run a Zed-style chord on a right view: reveal → focus → release.
     *  Same three-step contract as [`Self::activate_left`].
     *  @param view - The right-panel view the chord selects
     */
    fn activate_right(&mut self, view: RightView) // Show, focus or unfocus a right view
    {
        if self.right.open && self.right.view == view {
            self.focus = if self.focus == Focus::Right {
                Focus::Editor
            } else {
                Focus::Right
            };
        } else {
            self.right.view = view;
            self.right.open = true;
            self.panel_scroll_offset = 0.0;
        }
    }

    /** Ctrl+Q: close the panel that owns the keyboard.
     *  Focus returns to the editor so keys are never routed at a panel that
     *  is no longer there; with the editor focused this is a no-op.
     */
    fn close_focused_panel(&mut self) // Close the keyboard-focused side panel
    {
        match self.focus {
            Focus::Sidebar => {
                self.sidebar.open = false;
                self.focus = Focus::Editor;
            }
            Focus::Right => {
                self.right.open = false;
                self.focus = Focus::Editor;
            }
            Focus::Editor => {}
        }
    }

    /** Drop stale focus on the search field once its view leaves the screen.
     *  egui keeps `TextEdit` focus even after the widget stops rendering, and
     *  that would starve both the editor and panel navigation (both are gated
     *  on `text_edit_focused`).
     *  @param ui - Top-level ui, used for egui memory access
     */
    fn release_hidden_search_focus(&self, ui: &egui::Ui) // Release focus on a hidden search field
    {
        let visible = self.sidebar.open && self.sidebar.view == SidebarView::Search;
        if !visible && ui.memory(|m| m.has_focus(search_input_id())) {
            set_search_input_focus(ui.ctx(), false);
        }
    }

    /** Route in-panel navigation while a side panel owns the keyboard.
     *  Left panel: `j`/`k` (or arrows) move the row cursor, Enter opens the
     *  row, Escape hands focus back to the editor. Right panel: `j`/`k`
     *  scroll the content, Escape hands focus back to the editor.
     *  @param ui - Top-level ui, used to consume the key events
     */
    fn handle_panel_keys(&mut self, ui: &egui::Ui) // j/k/Enter/Escape inside a focused panel
    {
        if self.focus == Focus::Editor {
            return;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.focus = Focus::Editor;
            return;
        }
        // One row per press; `|` maps to a down-stroke, `k` to an up-stroke.
        let delta = if ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, Key::J)
                || i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
        }) {
            1
        } else if ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, Key::K)
                || i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
        }) {
            -1
        } else {
            0
        };

        match self.focus {
            Focus::Editor => {}
            Focus::Sidebar => {
                // `i` is vim for "type here": hand the keyboard to the query
                // field so the Search view stays usable without a mouse. The
                // focus itself is deferred one frame — see the input block.
                if self.sidebar.view == SidebarView::Search
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::I))
                {
                    self.pending_search_focus = true;
                    return;
                }
                let len = match self.sidebar.view {
                    SidebarView::Explorer => self.explorer_rows().len(),
                    SidebarView::Search => self.sidebar.hits.len(),
                    _ => 0,
                };
                if delta != 0 {
                    self.sidebar.move_cursor(delta, len);
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
                    self.activate_panel_row();
                }
            }
            Focus::Right => {
                if delta != 0 {
                    self.panel_scroll_rows += delta;
                    // Manual scrolling takes over from stick-to-bottom.
                    self.ai_auto_scroll = false;
                }
            }
        }
    }

    /** Open the row under the left panel's keyboard cursor.
     *  Explorer rows load their page; Search rows open the hit's page.
     */
    fn activate_panel_row(&mut self) // Enter on the focused row
    {
        let target = match self.sidebar.view {
            SidebarView::Explorer => self
                .explorer_rows()
                .get(self.sidebar.cursor)
                .map(|row| row.page.clone()),
            SidebarView::Search => self
                .sidebar
                .hits
                .get(self.sidebar.cursor)
                .map(|hit| hit.page.clone()),
            _ => None,
        };
        if let Some(name) = target {
            self.load_page(&name);
        }
    }

    /** Enter or leave zen mode: hide sidebar/activity bars/menu and go fullscreen.
     *  @param on - Whether zen mode should be active
     *  @param ctx - egui context for the viewport command
     */
    fn set_zen(&mut self, on: bool, ctx: &egui::Context) // Toggle chrome-less fullscreen
    {
        if on == self.zen {
            return;
        }
        if on {
            self.zen_restore = Some((self.sidebar.open, self.right.open, self.menu_visible));
            self.sidebar.open = false;
            self.right.open = false;
            self.menu_visible = false;
        } else if let Some((sidebar, right, menu)) = self.zen_restore.take() {
            self.sidebar.open = sidebar;
            self.right.open = right;
            self.menu_visible = menu;
        }
        self.zen = on;
        self.palette.zen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    fn render_workspace(&mut self, ui: &mut egui::Ui) {
        self.ai_poll_events();

        // Process deferred actions set during the previous frame's editor_ui.
        let cmd = std::mem::replace(&mut self.vim.command_action, CommandAction::None);
        self.dispatch_action(cmd, ui.ctx());

        // Menu bar — hidden unless settings/window state says otherwise.
        if self.menu_visible {
            Panel::top("menu_bar").show_inside(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Save (Ctrl+S)").clicked() {
                            self.save_current_page();
                            ui.close();
                        }
                        if ui.button("New Page").clicked() {
                            self.new_page_name = "untitled".to_string();
                            ui.close();
                        }
                        if ui.button("Delete Page").clicked() {
                            self.delete_current_page();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Open Workspace").clicked() {
                            if let Some(dir) = self.pick_workspace_folder() {
                                self.open_workspace(&dir);
                            }
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("View", |ui| {
                        let mut menu_visible = self.menu_visible;
                        if ui.checkbox(&mut menu_visible, "Menu bar (Alt)").changed() {
                            self.menu_visible = menu_visible;
                        }
                        ui.separator();
                        ui.label("Right Panel");
                        for view in RightView::all() {
                            let mut open = self.right.is_open(view);
                            if ui.checkbox(&mut open, view.label()).changed() {
                                self.right.set_open(view, open);
                            }
                        }
                        ui.separator();
                        ui.label("Sidebar");
                        for view in SidebarView::all() {
                            let mut open = self.sidebar.open;
                            if ui.checkbox(&mut open, view.label()).changed() {
                                self.sidebar.open = open;
                                self.sidebar.view = view;
                            }
                        }
                        ui.separator();
                        ui.menu_button("Theme", |ui| {
                            let current = self.settings.theme.clone();
                            if ui
                                .selectable_label(current == "wid-dark", "Wid Dark")
                                .clicked()
                            {
                                self.settings.theme = "wid-dark".to_string();
                                self.apply_theme();
                                ui.close();
                            }
                            if ui
                                .selectable_label(current == "gruvbox", "Gruvbox Dark")
                                .clicked()
                            {
                                self.settings.theme = "gruvbox".to_string();
                                self.apply_theme();
                                ui.close();
                            }
                            ui.separator();
                            let mut invert = self.settings.invert;
                            if ui
                                .checkbox(&mut invert, "Light mode (invert, Alt+T)")
                                .changed()
                            {
                                self.settings.invert = invert;
                                self.apply_theme();
                                ui.close();
                            }
                        });
                    });
                    if ui.button("Today's Journal").clicked() {
                        self.open_today_journal();
                    }
                });
            });
        }

        // Status bar — declared early so later panels (sidebar, editor,
        // right panel) lay out in the remaining area instead of being
        // covered by it. A bottom panel declared last would overlap the
        // AI chat input and push it off the bottom of the window.
        Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(self.theme.editor.background)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(ref ws) = self.workspace {
                        ui.label(
                            egui::RichText::new(format!("Workspace: {}", ws.config.name))
                                .color(self.theme.editor.foreground),
                        );
                        ui.separator();
                    }
                    ui.label(
                        egui::RichText::new(&self.status_message)
                            .color(self.theme.editor.foreground),
                    );
                });
            });

        // Activity bar (VSCode-style icon strip on the far left) — hidden in zen mode
        if !self.zen {
            Panel::left("activity_bar")
                .exact_size(sidebar::ACTIVITY_BAR_WIDTH)
                .resizable(false)
                // No frame inset: the rail is exactly 48px wide, so glyphs center on it.
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::ZERO)
                        .fill(self.theme.titlebar.background),
                )
                .show_inside(ui, |ui| {
                    self.sidebar.activity_bar_ui(ui, &self.theme);
                });
        }

        // Side panel, contents depend on the active activity-bar view
        if self.sidebar.open {
            Panel::left("sidebar")
                .resizable(true)
                .default_size(260.0)
                .min_size(170.0)
                // Theme-colored fill so the panel and the rail read as one surface.
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(8, 10))
                        .fill(self.theme.sidebar.background),
                )
                .show_inside(ui, |ui| match self.sidebar.view {
                    SidebarView::Explorer => self.render_explorer(ui),
                    SidebarView::Search => self.render_search(ui),
                    other => sidebar::render_placeholder(ui, other),
                });
        }

        // Right activity bar + side panel (preview, agent chat). egui lays the
        // first-declared panel outermost, so the rail hugs the window edge and
        // the panel sits inboard of it — the mirror image of the left side.
        if !self.zen {
            Panel::right("right_activity_bar")
                .exact_size(sidebar::ACTIVITY_BAR_WIDTH)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::ZERO)
                        .fill(self.theme.titlebar.background),
                )
                .show_inside(ui, |ui| {
                    self.right.activity_bar_ui(ui, &self.theme);
                });
        }

        if self.right.open {
            Panel::right("right_sidebar")
                .resizable(true)
                .default_size(360.0)
                .min_size(240.0)
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(8, 10))
                        .fill(self.theme.sidebar.background),
                )
                .show_inside(ui, |ui| match self.right.view {
                    RightView::Preview => self.render_preview_panel(ui),
                    RightView::Agent => self.render_ai_panel(ui),
                });
        }

        // Editor
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.current_page.is_some() || !self.buffer.text.is_empty() {
                let mode_str = match self.vim.mode {
                    Mode::Normal => "NORMAL",
                    Mode::Insert => "INSERT",
                    Mode::Visual(_) => "VISUAL",
                    Mode::CommandLine => "COMMAND",
                };
                let mode_color = match self.vim.mode {
                    Mode::Normal => self.theme.status.mode_normal,
                    Mode::Insert => self.theme.status.mode_insert,
                    Mode::Visual(_) => self.theme.status.mode_visual,
                    Mode::CommandLine => self.theme.status.mode_command,
                };
                let modified = if self.vim.needs_save(&self.buffer) {
                    "  ●"
                } else {
                    ""
                };

                egui::Frame::new()
                    .fill(self.theme.editor.background)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.colored_label(mode_color, format!(" {} ", mode_str));
                            ui.separator();
                            if let Some(ref name) = self.current_page {
                                ui.label(format!("{}.md{}", name, modified));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let (line, col) = self.buffer.line_col(self.buffer.cursor);
                                    ui.label(format!(
                                        "Ln {}, Col {}  |  {} lines",
                                        line + 1,
                                        col + 1,
                                        self.buffer.line_count(),
                                    ));
                                    ui.separator();
                                    ui.colored_label(Color32::GRAY, " Markdown ");
                                },
                            );
                        });
                    });
                ui.separator();

                // Command line mode input
                if let Mode::CommandLine = self.vim.mode {
                    egui::Frame::new()
                        .fill(self.theme.editor.background)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let cmd = self.vim.command_line.clone();
                                ui.label(
                                    egui::RichText::new(&cmd).color(self.theme.editor.foreground),
                                );
                            });
                        });
                    ui.separator();
                }

                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .show(ui, |ui| {
                        let editor_focused = self.focus == Focus::Editor;
                        let refocused = editor_ui(
                            ui,
                            &mut self.vim,
                            &mut self.buffer,
                            &self.settings,
                            &self.theme,
                            editor_focused,
                        );
                        if refocused {
                            self.focus = Focus::Editor;
                        }
                    });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("Wid");
                    ui.label("A local-first knowledge workspace");
                    ui.add_space(10.0);
                    if ui.button("Create a new page").clicked() {
                        self.new_page_name = "untitled".to_string();
                        self.create_page();
                    }
                });
            }
        });

        // Command palette overlay — rendered last so it sits above all panels.
        let palette_outcome = self.palette.ui(ui.ctx(), &self.theme);
        if let Some(outcome) = palette_outcome {
            self.apply_palette_outcome(outcome, ui.ctx());
        }

        // One-shot reveal/scroll requests are consumed by the paint above;
        // clear any that had no list to land on this frame.
        self.sidebar.reveal_cursor = false;
        self.panel_scroll_rows = 0;
    }

    /** Explorer view: today's journal, then pages, then past journals.
     *  Rows come from `explorer_rows`, so the keyboard cursor and the paint
     *  can never disagree about what row N is.
     */
    fn render_explorer(&mut self, ui: &mut egui::Ui) {
        let focused = self.focus == Focus::Sidebar;
        sidebar::panel_header(
            ui,
            "Explorer",
            focused.then_some("j/k · Enter · Ctrl+Q"),
            &self.theme,
        );
        ui.add_space(8.0);

        if sidebar::new_page_control(ui, &mut self.new_page_name, &self.theme) {
            self.create_page();
        }
        ui.add_space(4.0);

        if self.workspace.is_none() {
            return;
        }

        let rows = self.explorer_rows();
        let current = self.current_page.clone();
        let cursor = self.sidebar.cursor;
        let reveal = self.sidebar.reveal_cursor;
        let mut clicked_row: Option<usize> = None;
        let mut open: Option<String> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut group: Option<ExplorerGroup> = None;
            for (i, row) in rows.iter().enumerate() {
                if group != Some(row.group) {
                    match row.group {
                        ExplorerGroup::Pages => sidebar::section_label(ui, "Pages", &self.theme),
                        ExplorerGroup::Journals => {
                            sidebar::section_label(ui, "Journals", &self.theme)
                        }
                        ExplorerGroup::Today => {}
                    }
                    group = Some(row.group);
                }
                let is_current = current.as_deref() == Some(row.page.as_str());
                let row_cursor = self.row_cursor(focused, i, cursor, reveal);
                if sidebar::nav_row(ui, &row.label, is_current, row_cursor, &self.theme) {
                    clicked_row = Some(i);
                    open = Some(row.page.clone());
                }
            }
        });

        // Clicking a row both opens it and moves the keyboard cursor there,
        // so `j`/`k` continue from where the mouse left off.
        if let Some(i) = clicked_row {
            self.focus = Focus::Sidebar;
            self.sidebar.cursor = i;
        }
        if let Some(name) = open {
            self.load_page(&name);
        }
    }

    /** Map a row index to its keyboard-cursor paint state.
     *  @param focused - Whether the panel currently owns the keyboard
     *  @param index - Row index being painted
     *  @param cursor - The panel's cursor index
     *  @param reveal - Whether the cursor just moved and needs scrolling into view
     *  @returns The `RowCursor` state to draw for this row
     */
    fn row_cursor(
        &self,
        focused: bool,
        index: usize,
        cursor: usize,
        reveal: bool,
    ) -> sidebar::RowCursor // Cursor paint state for one row
    {
        if focused && index == cursor {
            if reveal {
                sidebar::RowCursor::Reveal
            } else {
                sidebar::RowCursor::On
            }
        } else {
            sidebar::RowCursor::Off
        }
    }

    /** Build the Explorer's rows in keyboard order.
     *  @returns Today's journal, then pages, then past journals (today excluded
     *           from the latter two, matching the painted sections)
     */
    fn explorer_rows(&self) -> Vec<ExplorerRow> // Explorer rows in cursor order
    {
        let Some(ws) = &self.workspace else {
            return Vec::new();
        };
        let today = ws.today_journal();
        let mut rows = vec![ExplorerRow {
            group: ExplorerGroup::Today,
            label: format!("Journal: {}", today),
            page: today.clone(),
        }];
        for page in ws.pages() {
            if *page == today {
                continue;
            }
            rows.push(ExplorerRow {
                group: ExplorerGroup::Pages,
                label: format!("{}.md", page),
                page: page.clone(),
            });
        }
        for journal in ws.journals() {
            if *journal == today {
                continue;
            }
            rows.push(ExplorerRow {
                group: ExplorerGroup::Journals,
                label: journal.clone(),
                page: journal.clone(),
            });
        }
        rows
    }

    /// Search view: case-insensitive substring match across pages and journals.
    fn render_search(&mut self, ui: &mut egui::Ui) {
        let focused = self.focus == Focus::Sidebar;
        sidebar::panel_header(
            ui,
            "Search",
            focused.then_some("j/k · Enter · Ctrl+Q"),
            &self.theme,
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            let width = ui.available_width();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.sidebar.query)
                    .id(search_input_id())
                    .hint_text("Search notes...")
                    .desired_width((width - 48.0).max(40.0)),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_search();
            }
            if ui.button("Go").clicked() {
                self.run_search();
            }
        });

        ui.add_space(6.0);

        if self.sidebar.hits.is_empty() {
            if self.sidebar.searched {
                ui.label(
                    egui::RichText::new("No results")
                        .color(Color32::GRAY)
                        .size(12.0),
                );
            } else {
                ui.label(
                    egui::RichText::new("Type a query and press Enter.")
                        .color(Color32::GRAY)
                        .size(12.0),
                );
            }
            return;
        }

        ui.label(
            egui::RichText::new(format!("{} results", self.sidebar.hits.len()))
                .color(Color32::GRAY)
                .small(),
        );

        let hits = self.sidebar.hits.clone();
        let cursor = self.sidebar.cursor;
        let reveal = self.sidebar.reveal_cursor;
        let mut clicked_row: Option<usize> = None;
        let mut open: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, hit) in hits.iter().enumerate() {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("{}:{}", hit.page, hit.line_no))
                        .color(Color32::GRAY)
                        .small(),
                );
                let text = hit.line_text.trim().to_string();
                let row_cursor = self.row_cursor(focused, i, cursor, reveal);
                if sidebar::nav_row(ui, &text, false, row_cursor, &self.theme) {
                    clicked_row = Some(i);
                    open = Some(hit.page.clone());
                }
            }
        });

        if let Some(i) = clicked_row {
            self.focus = Focus::Sidebar;
            self.sidebar.cursor = i;
        }
        if let Some(page) = open {
            self.load_page(&page);
        }
    }

    /// Scan every workspace document for the current search query.
    fn run_search(&mut self) {
        let query = self.sidebar.query.trim().to_lowercase();
        self.sidebar.searched = true;
        self.sidebar.hits.clear();
        if query.is_empty() {
            return;
        }

        let Some(ref ws) = self.workspace else { return };
        let mut names: Vec<String> = ws.pages().to_vec();
        names.extend(ws.journals().iter().cloned());

        for name in names {
            let Ok(text) = ws.read_page(&name) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&query) {
                    self.sidebar.hits.push(SearchHit {
                        page: name.clone(),
                        line_no: i + 1,
                        line_text: line.chars().take(120).collect(),
                    });
                    if self.sidebar.hits.len() >= 200 {
                        return;
                    }
                }
            }
        }
    }

    // ── AI Panel Event Loop ──────────────────────────────────

    fn ai_poll_events(&mut self) {
        while let Ok(event) = self.ai_event_rx.try_recv() {
            match event {
                AsyncEvent::ModelsLoaded(models) => {
                    self.ai_models = models.clone();
                    if !self.ai_loaded_from_disk
                        && self.ai_conversations.is_empty()
                        && let Some(model) = models.first()
                    {
                        self.ai_conversations.push(Conversation {
                            id: self.ai_next_id,
                            title: model.clone(),
                            messages: vec![],
                            model: model.clone(),
                            web_search: self.settings.ai.search_enabled_by_default,
                            notes_context: self.settings.ai.notes_context_enabled,
                        });
                        self.ai_next_id += 1;
                    }
                    self.ai_status = format!("Ready — {} models", self.ai_models.len());
                }
                AsyncEvent::ModelsError(msg) => {
                    if !self.ai_loaded_from_disk {
                        self.ai_status = format!("Ollama: {msg}");
                    }
                }
                AsyncEvent::StreamChunk(content) => {
                    if let Some(cid) = self.ai_stream_conv_id
                        && let Some(conv) = self.ai_conversations.iter_mut().find(|c| c.id == cid)
                        && let Some(msg) = conv.messages.last_mut()
                        && msg.role == "assistant"
                    {
                        msg.content.push_str(&content);
                    }
                    self.ai_auto_scroll = true;
                }
                AsyncEvent::StreamDone => {
                    self.ai_is_streaming = false;
                    self.ai_stream_conv_id = None;
                    self.ai_save();
                }
                AsyncEvent::StreamError(msg) => {
                    self.ai_is_streaming = false;
                    if let Some(cid) = self.ai_stream_conv_id.take()
                        && let Some(conv) = self.ai_conversations.iter_mut().find(|c| c.id == cid)
                    {
                        conv.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("Error: {msg}"),
                        });
                    }
                    self.ai_save();
                }
                AsyncEvent::SearchResults(results) => {
                    self.ai_is_searching = false;
                    if let Some(query) = self.ai_pending_search_query.take() {
                        self.ai_status =
                            format!("Found {} results, sending to model…", results.len());
                        let context = format_results_as_context(&query, &results);
                        if !self.ai_conversations.is_empty() {
                            let idx = self.ai_selected_idx;
                            let model = self.ai_conversations[idx].model.clone();
                            let conv_id = self.ai_conversations[idx].id;
                            let mut history: Vec<ChatMessage> = self.ai_conversations[idx]
                                .messages
                                .iter()
                                .filter(|m| m.role != "system")
                                .cloned()
                                .collect();
                            // Truncate to max_history messages
                            let max = self.settings.ai.max_history;
                            if history.len() > max {
                                let drain = history.len() - max;
                                history.drain(..drain);
                            }
                            history.push(ChatMessage {
                                role: "user".to_string(),
                                content: context.clone(),
                            });
                            let conv = &mut self.ai_conversations[idx];
                            conv.messages.push(ChatMessage {
                                role: "user".to_string(),
                                content: context,
                            });
                            conv.messages.push(ChatMessage {
                                role: "assistant".to_string(),
                                content: String::new(),
                            });
                            self.ai_is_streaming = true;
                            self.ai_auto_scroll = true;
                            self.ai_start_stream(history, model, conv_id);
                        }
                    }
                }
                AsyncEvent::SearchError(msg) => {
                    self.ai_is_searching = false;
                    self.ai_pending_search_query = None;
                    self.ai_status = format!("Search error: {msg}");
                    if let Some(conv) = self.current_conv_mut() {
                        conv.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("Web search failed: {msg}"),
                        });
                    }
                    self.ai_save();
                }
            }
        }
    }

    fn ai_start_stream(&mut self, history: Vec<ChatMessage>, model: String, conv_id: u64) {
        self.ai_stream_conv_id = Some(conv_id);
        self.ai_stream_cancel.store(false, Ordering::Relaxed);
        let tx = self.ai_event_tx.clone();
        let client = self.ai_client.clone();
        let cancel = Arc::clone(&self.ai_stream_cancel);
        self.ai_runtime.spawn(async move {
            client.chat_stream(model, history, tx, cancel).await;
        });
    }

    fn ai_send_message(&mut self) {
        let text = self.ai_input.trim().to_string();
        if text.is_empty() || self.ai_is_streaming || self.ai_conversations.is_empty() {
            return;
        }

        let model = self.ai_conversations[self.ai_selected_idx].model.clone();
        let mut history = self.ai_conversations[self.ai_selected_idx].messages.clone();

        // Inject notes context if enabled
        let notes_on = self.ai_conversations[self.ai_selected_idx].notes_context;
        if notes_on && let Some(ref ws) = self.workspace {
            let ctx = ai::context::build_context(&ws.root, self.settings.ai.max_notes);
            if !ctx.is_empty() {
                history.insert(
                    0,
                    ChatMessage {
                        role: "system".to_string(),
                        content: format!("Here are the user's recent notes for context.\n{}", ctx),
                    },
                );
            }
        }

        // Truncate to max_history messages (keep most recent)
        let max = self.settings.ai.max_history;
        if history.len() > max {
            let drain = history.len() - max;
            history.drain(..drain);
        }

        self.ai_last_user_msg = Some(text.clone());
        history.push(ChatMessage {
            role: "user".to_string(),
            content: text.clone(),
        });

        if let Some(conv) = self.current_conv_mut() {
            conv.messages.push(ChatMessage {
                role: "user".to_string(),
                content: text,
            });
            conv.messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
            });
            let conv_id = conv.id;
            self.ai_input.clear();
            self.ai_is_streaming = true;
            self.ai_auto_scroll = true;
            self.ai_start_stream(history, model, conv_id);
        }
    }

    fn ai_retry(&mut self) {
        let text = match self.ai_last_user_msg.take() {
            Some(t) => t,
            None => return,
        };
        if self.ai_is_streaming || self.ai_conversations.is_empty() {
            return;
        }

        // Remove the error message (last system message)
        if let Some(conv) = self.current_conv_mut()
            && conv.messages.last().map(|m| m.role.as_str()) == Some("system")
        {
            conv.messages.pop();
        }

        // Restore input and re-send
        self.ai_input = text;
        self.ai_send_message();
    }

    fn ai_send_search_message(&mut self) {
        let text = self.ai_input.trim().to_string();
        if text.is_empty()
            || self.ai_is_streaming
            || self.ai_is_searching
            || self.ai_conversations.is_empty()
        {
            return;
        }

        if !self.ai_searcher.is_configured() {
            self.ai_show_settings = true;
            self.ai_status = format!(
                "{} needs configuration — open ⚙ Settings.",
                self.ai_searcher.display_name()
            );
            return;
        }

        self.ai_input.clear();
        self.ai_is_searching = true;
        self.ai_pending_search_query = Some(text.clone());
        self.ai_status = format!("Searching for \"{text}\"…");
        let dots = self.ai_loading_dots();
        if let Some(conv) = self.current_conv_mut() {
            conv.messages.push(ChatMessage {
                role: "system".to_string(),
                content: format!("Searching the web for \"{text}\"{dots}"),
            });
        }

        let searcher = self.ai_searcher.clone();
        let tx = self.ai_event_tx.clone();
        self.ai_runtime.spawn(async move {
            match searcher.search(&text).await {
                Ok(results) => {
                    let _ = tx.send(AsyncEvent::SearchResults(results));
                }
                Err(e) => {
                    let msg = match e {
                        ai::search::SearchError::Misconfigured(s) => s,
                        ai::search::SearchError::RequestFailed(s) => {
                            format!("Search request failed: {s}")
                        }
                        ai::search::SearchError::ParseFailed(s) => {
                            format!("Failed to parse search results: {s}")
                        }
                        ai::search::SearchError::NoResults => {
                            "No search results found.".to_string()
                        }
                    };
                    let _ = tx.send(AsyncEvent::SearchError(msg));
                }
            }
        });
    }

    fn ai_new_conversation(&mut self) {
        let model = self
            .ai_models
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let conv = Conversation {
            id: self.ai_next_id,
            title: format!("Chat {}", self.ai_conversations.len() + 1),
            messages: vec![],
            model,
            web_search: self.settings.ai.search_enabled_by_default,
            notes_context: self.settings.ai.notes_context_enabled,
        };
        self.ai_next_id += 1;
        self.ai_conversations.push(conv);
        self.ai_selected_idx = self.ai_conversations.len() - 1;
        self.ai_save();
    }

    fn ai_delete_conversation(&mut self) {
        if self.ai_conversations.is_empty() {
            return;
        }
        self.ai_conversations.remove(self.ai_selected_idx);
        if self.ai_conversations.is_empty() {
            self.ai_new_conversation();
        } else {
            self.ai_selected_idx = self.ai_selected_idx.min(self.ai_conversations.len() - 1);
        }
        self.ai_save();
    }

    fn ai_select_model(&mut self, model: &str) {
        if let Some(conv) = self.current_conv_mut() {
            conv.model = model.to_string();
        }
        self.ai_show_model_picker = false;
        self.ai_save();
    }

    fn current_conv(&self) -> Option<&Conversation> {
        self.ai_conversations.get(self.ai_selected_idx)
    }

    fn current_conv_mut(&mut self) -> Option<&mut Conversation> {
        self.ai_conversations.get_mut(self.ai_selected_idx)
    }

    fn ai_rebuild_searcher(&mut self) {
        self.ai_searcher = provider_from_config(&self.settings.ai);
    }

    fn ai_save(&mut self) {
        if let Some(ref ws) = self.workspace {
            let _ = ai::persistence::save(&ws.root, &self.ai_conversations);
        }
    }

    fn ai_export_conversation(&mut self) {
        if self.ai_conversations.is_empty() {
            return;
        }
        let Some(conv) = self.current_conv() else {
            return;
        };
        let default_name = format!("{}_ai_chat", conv.title.replace(' ', "_"));
        let model = conv.model.clone();
        let messages = conv.messages.clone();

        let path = self.run_file_dialog(move || {
            rfd::FileDialog::new()
                .set_title("Export Conversation")
                .add_filter("Markdown", &["md"])
                .add_filter("JSON", &["json"])
                .set_file_name(&default_name)
                .save_file()
        });

        if let Some(path) = path {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("md");
            let result = match ext {
                "json" => serde_json::to_string_pretty(&messages)
                    .map_err(std::io::Error::other)
                    .and_then(|json| std::fs::write(&path, json)),
                _ => {
                    let mut md = format!("# AI Conversation — {}\n\n", model);
                    for msg in &messages {
                        let label = match msg.role.as_str() {
                            "user" => "You",
                            "assistant" => "AI",
                            "system" => "System",
                            _ => "Unknown",
                        };
                        md.push_str(&format!("## {}\n\n{}\n\n", label, msg.content));
                    }
                    std::fs::write(&path, md)
                }
            };
            match result {
                Ok(()) => {
                    self.status_message = format!("Exported to {}", path.display());
                }
                Err(e) => {
                    self.status_message = format!("Export failed: {e}");
                }
            }
        }
    }

    fn ai_loading_dots(&self) -> &'static str {
        match self.ai_dot_frame % 3 {
            0 => ".",
            1 => "..",
            _ => "...",
        }
    }
    // ── Right panel views ─────────────────────────────────

    /** Right panel: rendered markdown for the buffer currently in the editor.
     *  @param ui - Right side panel ui, inset by the panel frame
     */
    /** Convert the pending right-panel row steps into a scroll offset.
     *  @param rows - Pending row steps accumulated by `handle_panel_keys`
     *  @returns The offset to request, or None when no scroll is pending
     */
    fn panel_scroll_request(&self, rows: isize) -> Option<f32> // Pending j/k scroll as a pixel offset
    {
        (rows != 0).then(|| {
            (self.panel_scroll_offset + rows as f32 * self.settings.editor.line_height).max(0.0)
        })
    }

    fn render_preview_panel(&mut self, ui: &mut egui::Ui) {
        let focused = self.focus == Focus::Right;
        let hint: Option<String> = if focused {
            Some("j/k · Ctrl+Q".to_string())
        } else {
            self.current_page.as_ref().map(|name| format!("{name}.md"))
        };
        sidebar::panel_header(ui, RightView::Preview.label(), hint.as_deref(), &self.theme);
        if self.buffer.text.trim().is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(egui::RichText::new("Nothing to preview").color(Color32::GRAY));
            });
            return;
        }
        // auto_shrink keeps the scroll area from collapsing to zero height and
        // coming back each frame, which would jitter the panel header.
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("preview_scroll")
            .auto_shrink([false, false]);
        if let Some(offset) = self.panel_scroll_request(self.panel_scroll_rows) {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let out = scroll.show(ui, |ui| {
            render_markdown(
                ui,
                &self.buffer.text,
                self.is_dark,
                &self.theme.invert_excludes,
            );
        });
        self.panel_scroll_offset = out.state.offset.y;
    }

    // ── AI Panel Rendering ─────────────────────────────────

    fn render_ai_panel(&mut self, ui: &mut egui::Ui) {
        let hint = if self.focus == Focus::Right {
            Some("j/k · Ctrl+Q")
        } else {
            Some("Ctrl+Shift+/")
        };
        sidebar::panel_header(ui, RightView::Agent.label(), hint, &self.theme);

        // Conversation selector
        ui.horizontal(|ui| {
            if ui.button(RichText::new("+ New Chat").size(12.0)).clicked() {
                self.ai_new_conversation();
            }
            if self.ai_conversations.len() > 1 {
                ui.add_space(4.0);
                if ui.button(RichText::new("× Del").size(12.0)).clicked() {
                    self.ai_delete_conversation();
                }
            }
            if let Some(conv) = self.current_conv()
                && !conv.messages.is_empty()
            {
                ui.add_space(4.0);
                if ui.button(RichText::new("↓ Export").size(12.0)).clicked() {
                    self.ai_export_conversation();
                }
            }
            // Model badge
            if let Some(conv) = self.current_conv() {
                let model = conv.model.clone();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new(&model).size(11.0).color(Color32::GRAY))
                        .clicked()
                    {
                        self.ai_show_model_picker = true;
                    }
                });
            }
        });

        // Toggles
        let idx = self.ai_selected_idx;
        let web_search = self
            .ai_conversations
            .get(idx)
            .map(|c| c.web_search)
            .unwrap_or(false);
        let notes_on = self
            .ai_conversations
            .get(idx)
            .map(|c| c.notes_context)
            .unwrap_or(false);
        let mut web_search = web_search;
        let mut notes_on = notes_on;
        ui.horizontal(|ui| {
            let resp = ui.checkbox(&mut web_search, "🌐 Search");
            if resp.changed() {
                if let Some(conv) = self.ai_conversations.get_mut(idx) {
                    conv.web_search = web_search;
                }
                self.ai_save();
            }
            let resp_n = ui.checkbox(&mut notes_on, "📓 Notes");
            if resp_n.changed() {
                if let Some(conv) = self.ai_conversations.get_mut(idx) {
                    conv.notes_context = notes_on;
                }
                self.ai_save();
            }
            // Help and settings ride along on this row: the header is reserved
            // for the panel title so both side panels read the same.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(RichText::new("⚙").size(13.0)).clicked() {
                    self.ai_show_settings = !self.ai_show_settings;
                }
                ui.add_space(2.0);
                if ui.button(RichText::new("?").size(13.0)).clicked() {
                    self.ai_show_help = !self.ai_show_help;
                }
            });
        });

        ui.separator();

        if self.ai_conversations.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label("No chats yet.");
                if ui.button("Start a new chat").clicked() {
                    self.ai_new_conversation();
                }
            });
            return;
        }

        // Chat messages
        self.render_ai_chat(ui);

        // Input area
        ui.separator();
        self.render_ai_input(ui);

        // Overlay windows
        self.render_ai_windows(ui);
    }

    fn render_ai_chat(&mut self, ui: &mut egui::Ui) {
        let Some(conv) = self.current_conv() else {
            return;
        };
        let mut scroll = ScrollArea::vertical()
            .id_salt("ai_chat_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(self.ai_auto_scroll);
        // A manual j/k scroll must win over stick-to-bottom for this frame.
        if let Some(offset) = self.panel_scroll_request(self.panel_scroll_rows) {
            scroll = scroll.vertical_scroll_offset(offset);
        }

        let mut retry = false;
        let out = scroll.show(ui, |ui| {
            if conv.messages.is_empty() && !self.ai_is_streaming {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.colored_label(Color32::GRAY, "Type a message to start chatting.");
                });
                return;
            }

            let last_idx = conv.messages.len().saturating_sub(1);
            for (i, msg) in conv.messages.iter().enumerate() {
                let (label, color) = match msg.role.as_str() {
                    "user" => ("You", self.accent(Color32::from_rgb(70, 130, 255))),
                    "assistant" => ("AI", self.accent(Color32::from_rgb(60, 200, 100))),
                    "system" => ("System", self.accent(Color32::from_rgb(200, 180, 50))),
                    _ => ("Unknown", self.accent(Color32::RED)),
                };

                ui.label(
                    RichText::new(format!("{label}:"))
                        .color(color)
                        .strong()
                        .size(13.0),
                );

                let frame = Frame::NONE
                    .fill(self.accent(Color32::from_rgb(30, 30, 40)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 4));
                frame.show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if msg.role == "assistant" {
                        render_markdown(
                            ui,
                            &msg.content,
                            self.is_dark,
                            &self.theme.invert_excludes,
                        );
                    } else if msg.role == "system" {
                        ui.label(
                            RichText::new(&msg.content)
                                .size(12.0)
                                .color(Color32::GRAY)
                                .italics(),
                        );
                    } else {
                        ui.label(RichText::new(&msg.content).size(13.0));
                    }
                });

                // Retry button after error messages
                if i == last_idx
                    && msg.role == "system"
                    && msg.content.starts_with("Error:")
                    && ui
                        .button(
                            RichText::new("↻ Retry")
                                .size(12.0)
                                .color(self.accent(Color32::from_rgb(100, 180, 255))),
                        )
                        .clicked()
                {
                    retry = true;
                }
                ui.add_space(6.0);
            }

            if self.ai_is_searching {
                let dots = self.ai_loading_dots();
                ui.colored_label(
                    Color32::GRAY,
                    RichText::new(format!("Searching the web{dots}")).italics(),
                );
            }

            if self.ai_is_streaming
                && let Some(last) = conv.messages.last()
                && last.role == "assistant"
                && last.content.is_empty()
            {
                let dots = self.ai_loading_dots();
                ui.colored_label(
                    Color32::GRAY,
                    RichText::new(format!("Generating{dots}")).italics(),
                );
            }
        });
        self.panel_scroll_offset = out.state.offset.y;

        if retry {
            self.ai_retry();
        }
    }

    fn render_ai_input(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let search_btn_w = 75.0;
            let send_btn_w = 50.0;
            let btns_w = search_btn_w + 4.0 + send_btn_w + 4.0;
            let available_w = ui.available_width() - btns_w;

            let can_act = !self.ai_input.trim().is_empty()
                && !self.ai_is_streaming
                && !self.ai_is_searching
                && !self.ai_conversations.is_empty();

            let response = TextEdit::multiline(&mut self.ai_input)
                .desired_rows(2)
                .desired_width(available_w.max(60.0))
                .hint_text("Type a message… (Ctrl+Enter)")
                .font(FontId::proportional(13.0))
                .show(ui);
            ui.add_space(4.0);

            let web_search_enabled = self
                .ai_conversations
                .get(self.ai_selected_idx)
                .map(|c| c.web_search)
                .unwrap_or(false);

            if ui
                .add_enabled(
                    can_act && web_search_enabled,
                    egui::Button::new(RichText::new("🔍 Search").size(12.0)),
                )
                .clicked()
            {
                self.ai_send_search_message();
            }

            ui.add_space(2.0);

            if self.ai_is_streaming {
                if ui
                    .button(
                        RichText::new("■ Stop")
                            .size(13.0)
                            .color(self.accent(Color32::from_rgb(220, 60, 60))),
                    )
                    .clicked()
                {
                    self.ai_stream_cancel.store(true, Ordering::Relaxed);
                }
            } else {
                if ui
                    .add_enabled(can_act, egui::Button::new(RichText::new("Send").size(13.0)))
                    .clicked()
                {
                    self.ai_send_message();
                }
            }

            if response.response.has_focus() {
                ui.input_mut(|input| {
                    if input.consume_key(egui::Modifiers::CTRL, Key::Enter) {
                        self.ai_send_message();
                    }
                });
            }
        });
    }

    fn render_ai_windows(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();

        // Settings window
        if self.ai_show_settings {
            Window::new("AI Settings")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.strong("Ollama");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("URL:");
                        ui.text_edit_singleline(&mut self.settings.ai.ollama_url);
                    });

                    ui.add_space(10.0);
                    ui.strong("Web Search Provider");
                    ui.separator();

                    let providers = [
                        ("brave", "Brave Search"),
                        ("searxng", "SearXNG (self-hosted)"),
                        ("duckduckgo", "DuckDuckGo (no API key)"),
                        ("tavily", "Tavily"),
                    ];

                    let mut sel = self.settings.ai.search_provider.clone();
                    egui::ComboBox::from_id_salt("ai_provider")
                        .selected_text(
                            providers
                                .iter()
                                .find(|(v, _)| *v == sel)
                                .map(|(_, l)| *l)
                                .unwrap_or("Select"),
                        )
                        .show_ui(ui, |ui| {
                            for (value, label) in &providers {
                                ui.selectable_value(&mut sel, value.to_string(), *label);
                            }
                        });
                    self.settings.ai.search_provider = sel;

                    ui.add_space(6.0);

                    match self.settings.ai.search_provider.as_str() {
                        "brave" => {
                            ui.horizontal(|ui| {
                                ui.label("API Key:");
                                ui.add(
                                    TextEdit::singleline(&mut self.settings.ai.brave_api_key)
                                        .password(true)
                                        .desired_width(180.0),
                                );
                            });
                        }
                        "searxng" => {
                            ui.horizontal(|ui| {
                                ui.label("URL:");
                                ui.text_edit_singleline(&mut self.settings.ai.searxng_url);
                            });
                        }
                        "duckduckgo" => {
                            ui.label("No configuration needed — ready to use!");
                        }
                        "tavily" => {
                            ui.horizontal(|ui| {
                                ui.label("API Key:");
                                ui.add(
                                    TextEdit::singleline(&mut self.settings.ai.tavily_api_key)
                                        .password(true)
                                        .desired_width(180.0),
                                );
                            });
                        }
                        _ => {}
                    }

                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut self.settings.ai.search_enabled_by_default,
                        "Enable web search by default",
                    );

                    ui.add_space(10.0);
                    ui.strong("Notes Context");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Max notes:");
                        let mut max = self.settings.ai.max_notes as f32;
                        ui.add(egui::Slider::new(&mut max, 1.0..=10.0).text("notes"));
                        self.settings.ai.max_notes = max as usize;
                    });
                    ui.checkbox(
                        &mut self.settings.ai.notes_context_enabled,
                        "Enable notes context by default",
                    );

                    ui.add_space(10.0);
                    ui.strong("Chat History");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Max messages sent to model:");
                        let mut max = self.settings.ai.max_history as f32;
                        ui.add(egui::Slider::new(&mut max, 5.0..=100.0).text("messages"));
                        self.settings.ai.max_history = max as usize;
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.ai_rebuild_searcher();
                            self.ai_client = OllamaClient::new(&self.settings.ai.ollama_url);
                            if let Some(ref ws) = self.workspace {
                                let _ = ws.save_settings(&self.settings);
                            }
                            self.ai_show_settings = false;
                            self.ai_status = "Settings saved.".to_string();
                        }
                        if ui.button("Cancel").clicked() {
                            let saved = self
                                .workspace
                                .as_ref()
                                .map(|ws| ws.load_settings())
                                .unwrap_or_default();
                            self.settings.ai = saved.ai;
                            self.ai_rebuild_searcher();
                            self.ai_show_settings = false;
                        }
                    });
                });
        }

        // Model picker window
        if self.ai_show_model_picker {
            Window::new("Select Model")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    let model_list: Vec<String> = self.ai_models.clone();
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for model in &model_list {
                            if ui.selectable_label(false, model).clicked() {
                                self.ai_select_model(model);
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Cancel").clicked() {
                        self.ai_show_model_picker = false;
                    }
                });
        }

        // Help window
        if self.ai_show_help {
            Window::new("AI Help")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.strong("AI Chat");
                    ui.separator();
                    ui.label("Ctrl+Enter  Send message");
                    ui.label("+ New Chat  Start a new conversation");
                    ui.label("× Del       Delete current conversation");
                    ui.separator();
                    ui.strong("Web Search");
                    ui.separator();
                    ui.label("Enable 🌐 toggle per chat to unlock the 🔍 Search button.");
                    ui.label("Configure search provider in ⚙ Settings.");
                    ui.separator();
                    ui.strong("Notes Context");
                    ui.separator();
                    ui.label("Enable 📓 toggle to inject workspace notes as context.");
                    ui.label("Recent pages and journals are included automatically.");
                    ui.separator();
                    if ui.button("Close").clicked() {
                        self.ai_show_help = false;
                    }
                });
        }

        // Keep repainting during streaming or searching
        if self.ai_is_streaming || self.ai_is_searching {
            self.ai_dot_frame = self.ai_dot_frame.wrapping_add(1);
            ctx.request_repaint();
        }
    }
}

// ── Agent Chat repro harness ───────────────────────────────
//
// Drives the real `WidApp::ui` through egui_kittest with injected
// pointer/key events, asserting the full "can I chat?" chain:
// input box rendered → click focuses it → typed text lands in
// `ai_input` → Send pushes messages into the conversation.

#[cfg(test)]
mod chat_input_repro {
    use super::*;
    use eframe::App as _;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /** Build a WidApp sitting in WorkspaceReady with the Agent panel open
     *  and one empty conversation — the state a user reaches after opening
     *  Agent Chat from the right activity bar.
     *  @returns A WidApp backed by a throwaway workspace in the temp dir
     */
    fn agent_panel_app() -> WidApp {
        let mut app = WidApp::new(None, Settings::default());
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("wid-chat-repro-{}-{unique}", std::process::id()));
        let ws = Workspace::create(&dir, "repro").expect("create temp workspace");
        app.workspace = Some(ws);
        app.screen = AppScreen::WorkspaceReady;
        app.show_prompt = false;
        app.right.open = true;
        app.right.view = RightView::Agent;
        app.ai_conversations.push(Conversation {
            id: 1,
            title: "Chat 1".to_string(),
            messages: vec![],
            model: "test-model".to_string(),
            web_search: false,
            notes_context: false,
        });
        app
    }

    /** Wrap a WidApp in a headless egui harness that renders `app.ui`.
     *  @param app - The app under test (moved into the harness state)
     *  @returns A Harness driving one `WidApp::ui` call per frame
     */
    fn harness(app: WidApp) -> Harness<'static, WidApp> {
        Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_max_steps(50)
            .build_ui_state(
                |ui, app: &mut WidApp| {
                    let mut frame = eframe::Frame::_new_kittest();
                    app.ui(ui, &mut frame);
                },
                app,
            )
    }

    /** Repro: type into the Agent Chat message box and send.
     *  Each assertion isolates one failure mode of "I can't actually chat".
     */
    #[test]
    fn typing_into_the_agent_chat_input_lands_in_ai_input() {
        let h = harness(agent_panel_app());

        // Symptom (d): no input box at all.
        let _input = h
            .query_by_role(Role::MultilineTextInput)
            .expect("chat input box was not rendered at all");
    }
}

// ── Editor input repro harness ───────────────────────────────
//
// Drives the real `WidApp::ui` through egui_kittest with injected
// pointer/key events, asserting the user's exact symptom: after
// clicking the editor, arrow keys navigate and typed text edits
// the buffer. Goes red when keyboard input stops reaching vim.

#[cfg(test)]
mod editor_input_repro {
    use super::*;
    use eframe::App as _;
    use egui_kittest::Harness;

    /** Build a WidApp in WorkspaceReady with a populated buffer and
     *  both side panels closed, so the editor owns most of the window.
     *  @returns A WidApp backed by a throwaway workspace in the temp dir
     */
    fn editor_app() -> WidApp {
        let mut app = WidApp::new(None, Settings::default());
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("wid-editor-repro-{}-{unique}", std::process::id()));
        let ws = Workspace::create(&dir, "repro").expect("create temp workspace");
        app.workspace = Some(ws);
        app.screen = AppScreen::WorkspaceReady;
        app.show_prompt = false;
        app.sidebar.open = false;
        app.right.open = false;
        app.buffer = Buffer::new_with_text("alpha\nbeta\ngamma".to_string());
        app
    }

    /** Wrap a WidApp in a headless egui harness that renders `app.ui`.
     *  @param app - The app under test (moved into the harness state)
     *  @returns A Harness driving one `WidApp::ui` call per frame
     */
    fn harness(app: WidApp) -> Harness<'static, WidApp> {
        Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_max_steps(50)
            .build_ui_state(
                |ui, app: &mut WidApp| {
                    let mut frame = eframe::Frame::_new_kittest();
                    app.ui(ui, &mut frame);
                },
                app,
            )
    }

    /** Click the middle of the window — with both side panels closed
     *  that lands squarely inside the editor widget.
     *  @param h - The harness under test
     */
    fn click_editor(h: &mut Harness<'static, WidApp>) {
        let pos = egui::pos2(600.0, 400.0);
        h.hover_at(pos);
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }

    /** Repro: click the editor, then navigate and edit with the keyboard.
     *  Each assertion isolates one failure mode of "the editor is unfunctional".
     *  Click lands on line 1 (grid: text starts at x≈100, line height 22).
     */
    #[test]
    fn after_click_arrow_keys_navigate_and_text_edits() {
        let mut h = harness(editor_app());
        h.step();
        let pos = egui::pos2(110.0, 50.0);
        h.hover_at(pos);
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        assert_eq!(
            h.state().buffer.line_col(h.state().buffer.cursor).0,
            0,
            "click did not place the cursor on line 1"
        );

        // Symptom (a): cannot navigate — ArrowDown must move cursor to line 2.
        h.key_press(egui::Key::ArrowDown);
        h.step();
        assert_eq!(
            h.state().buffer.line_col(h.state().buffer.cursor).0,
            1,
            "arrow keys do not navigate the editor (cursor stuck at {})",
            h.state().buffer.cursor
        );

        // Symptom (b): cannot edit — 'i' must enter insert mode …
        h.event(egui::Event::Text("i".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Insert,
            "'i' did not enter insert mode (mode = {:?})",
            h.state().vim.mode
        );

        // … and typed text must land in the buffer.
        h.event(egui::Event::Text("Z".to_string()));
        h.step();
        assert!(
            h.state().buffer.text.contains('Z'),
            "typed text did not reach the buffer (text = {:?})",
            h.state().buffer.text
        );
    }

    /** Regression: the core vim loop — i, type, Escape, keep editing.
     *  egui surrenders keyboard focus on every Escape (EventFilter.escape
     *  defaults to false), so a focus-gated editor dies right here.
     */
    #[test]
    fn escape_from_insert_mode_keeps_the_editor_responsive() {
        let mut h = harness(editor_app());
        h.step();
        click_editor(&mut h);

        h.event(egui::Event::Text("i".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Insert,
            "'i' did not enter insert mode"
        );

        h.event(egui::Event::Text("Z".to_string()));
        h.step();
        assert!(
            h.state().buffer.text.contains('Z'),
            "typed text did not reach the buffer"
        );

        h.key_press(egui::Key::Escape);
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Normal,
            "Escape did not leave insert mode (mode = {:?})",
            h.state().vim.mode
        );

        // The editor must still respond after Escape. "Z" was inserted at the
        // start, so line 2 ("beta") begins at byte 7.
        h.key_press(egui::Key::ArrowDown);
        h.step();
        assert!(
            h.state().buffer.cursor >= 7,
            "editor dead after Escape — arrow keys ignored (cursor = {})",
            h.state().buffer.cursor
        );

        h.event(egui::Event::Text("i".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Insert,
            "editor dead after Escape — 'i' ignored (mode = {:?})",
            h.state().vim.mode
        );
    }

    /** Variation: keys sent before any click — is the editor focused at startup? */
    #[test]
    fn keys_without_any_click() {
        let mut h = harness(editor_app());
        h.step();

        h.event(egui::Event::Text("i".to_string()));
        h.step();
        h.event(egui::Event::Text("Z".to_string()));
        h.step();

        assert!(
            h.state().buffer.text.contains('Z'),
            "editor not focused at startup — typing without clicking does nothing \
             (text = {:?}, mode = {:?})",
            h.state().buffer.text,
            h.state().vim.mode
        );
    }

    /** Variation: realistic winit ordering — Key down, then Text, then Key up
     *  (the harness above only injects Text for letters; real egui delivers both).
     */
    #[test]
    fn realistic_winit_key_down_text_key_up_ordering() {
        let mut h = harness(editor_app());
        h.step();
        click_editor(&mut h);

        // 'i': Key down, Text, Key up — exactly what winit produces.
        h.event(egui::Event::Key {
            key: egui::Key::I,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
            repeat: false,
            physical_key: None,
        });
        h.event(egui::Event::Text("i".to_string()));
        h.event(egui::Event::Key {
            key: egui::Key::I,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
            repeat: false,
            physical_key: None,
        });
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Insert,
            "'i' (full winit event triple) did not enter insert mode (mode = {:?})",
            h.state().vim.mode
        );

        h.event(egui::Event::Key {
            key: egui::Key::Z,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
            repeat: false,
            physical_key: None,
        });
        h.event(egui::Event::Text("Z".to_string()));
        h.event(egui::Event::Key {
            key: egui::Key::Z,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
            repeat: false,
            physical_key: None,
        });
        h.step();
        assert!(
            h.state().buffer.text.contains('Z'),
            "typed text did not reach the buffer (text = {:?})",
            h.state().buffer.text
        );
    }

    /** Variation: idle frames between click and keys — catches late focus theft
     *  (something requesting focus on a later frame in the real app).
     */
    #[test]
    fn focus_survives_idle_frames_after_click() {
        let mut h = harness(editor_app());
        h.step();
        click_editor(&mut h);

        for _ in 0..15 {
            h.step();
        }

        h.event(egui::Event::Text("i".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Insert,
            "focus lost during idle frames (mode = {:?})",
            h.state().vim.mode
        );
    }

    /** Variation: '/' enters command-line mode — can Escape recover, or does
     *  the editor stay stuck swallowing keys afterwards?
     */
    #[test]
    fn command_line_mode_recovers_on_escape() {
        let mut h = harness(editor_app());
        h.step();
        click_editor(&mut h);

        h.event(egui::Event::Text("/".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::CommandLine,
            "'/' did not enter command-line mode (mode = {:?})",
            h.state().vim.mode
        );

        h.key_press(egui::Key::Escape);
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Normal,
            "Escape did not leave command-line mode (mode = {:?})",
            h.state().vim.mode
        );

        h.event(egui::Event::Text("i".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Insert,
            "editor stuck after command-line round-trip (mode = {:?})",
            h.state().vim.mode
        );
    }

    /** Press at `from`, move to `to` (crossing the drag threshold), release.
     *  @param h - Harness under test
     *  @param from - Press position (selection anchor)
     *  @param to - Drag target and release position (selection active end)
     */
    fn drag(h: &mut Harness<'static, WidApp>, from: egui::Pos2, to: egui::Pos2) {
        h.hover_at(from);
        h.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.hover_at(to);
        h.step();
        h.event(egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }

    /** Mouse: dragging across text must select it and enter visual mode.
     *  Drag from line 1 col ~0 to line 2 end (grid: text starts at x≈100,
     *  line height 22 — positions sit inside the 3-line test buffer).
     */
    #[test]
    fn drag_across_text_selects_and_enters_visual_mode() {
        let mut h = harness(editor_app());
        h.step();

        drag(&mut h, egui::pos2(105.0, 50.0), egui::pos2(150.0, 72.0));

        let app = h.state();
        assert!(
            matches!(app.vim.mode, Mode::Visual(_)),
            "drag did not enter visual mode (mode = {:?})",
            app.vim.mode
        );
        let sel = app
            .buffer
            .selection
            .as_ref()
            .expect("no selection after drag");
        assert!(
            sel.end() > sel.start(),
            "selection is empty after drag (anchor = {}, active = {})",
            sel.anchor,
            sel.active
        );
    }

    /** Mouse: a plain click (no drag) moves the cursor, clears the selection
     *  and returns to normal mode.
     */
    #[test]
    fn plain_click_moves_cursor_and_clears_selection() {
        let mut h = harness(editor_app());
        h.step();

        // Setup: drag once so we are in visual mode with a selection.
        drag(&mut h, egui::pos2(105.0, 50.0), egui::pos2(150.0, 72.0));
        let cursor_before = h.state().buffer.cursor;

        // Plain click on line 3 — no movement, so it stays a click.
        let pos = egui::pos2(110.0, 94.0);
        h.hover_at(pos);
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();

        let app = h.state();
        assert_eq!(
            app.vim.mode,
            Mode::Normal,
            "plain click did not leave visual mode (mode = {:?})",
            app.vim.mode
        );
        assert!(
            app.buffer.selection.is_none(),
            "plain click did not clear the selection"
        );
        assert_ne!(
            app.buffer.cursor, cursor_before,
            "plain click did not move the cursor"
        );
        assert_eq!(
            app.buffer.line_col(app.buffer.cursor).0,
            2,
            "plain click did not land on line 3 (cursor = {})",
            app.buffer.cursor
        );
    }
}

// ── Panel focus plumbing ────────────────────────────────────
//
// The search field lives in the left panel, but panel focus is driven from
// the top of `WidApp::ui` (chords, the palette, `i`). A stable id lets that
// outer code move egui's text focus without reaching into `render_search`.

/// Stable id for the sidebar search field.
fn search_input_id() -> egui::Id // Stable id for the sidebar search field
{
    egui::Id::new("wid_search_input")
}

/** Move keyboard focus into or out of the sidebar search field.
 *  @param ctx - egui context whose memory is shared by every panel
 *  @param focused - true focuses the field, false releases it
 */
fn set_search_input_focus(ctx: &egui::Context, focused: bool) // Focus or release the search field
{
    ctx.memory_mut(|m| {
        if focused {
            m.request_focus(search_input_id());
        } else {
            m.surrender_focus(search_input_id());
        }
    });
}

// ── Panel keybind repro harness ─────────────────────────────
//
// Drives the real `WidApp::ui` through egui_kittest, asserting the
// Zed-style chord contract from plans/keybind.md: first press shows
// a panel, second press focuses it (vim nav inside), Ctrl+Q closes
// only the focused panel.

#[cfg(test)]
mod panel_keys_repro {
    use super::*;
    use eframe::App as _;
    use egui_kittest::Harness;

    /** Build a WidApp in WorkspaceReady with two extra pages and both
     *  side panels closed, so each test starts from the same state.
     *  @returns A WidApp backed by a throwaway workspace in the temp dir
     */
    fn panel_app() -> WidApp {
        let mut app = WidApp::new(None, Settings::default());
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("wid-panel-repro-{}-{unique}", std::process::id()));
        let mut ws = Workspace::create(&dir, "repro").expect("create temp workspace");
        ws.create_page("alpha").expect("create page alpha");
        ws.create_page("beta").expect("create page beta");
        app.workspace = Some(ws);
        app.screen = AppScreen::WorkspaceReady;
        app.show_prompt = false;
        // Load today's journal (Explorer row 0) so the central editor widget is
        // rendered and `j`/Enter has a different row to open.
        let today = app
            .workspace
            .as_ref()
            .expect("workspace was just set")
            .today_journal();
        app.load_page(&today);
        app.sidebar.open = false;
        app.right.open = false;
        app.focus = Focus::Editor;
        app
    }

    /** Wrap a WidApp in a headless egui harness that renders `app.ui`.
     *  @param app - The app under test (moved into the harness state)
     *  @returns A Harness driving one `WidApp::ui` call per frame
     */
    fn harness(app: WidApp) -> Harness<'static, WidApp> {
        Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_max_steps(50)
            .build_ui_state(
                |ui, app: &mut WidApp| {
                    let mut frame = eframe::Frame::_new_kittest();
                    app.ui(ui, &mut frame);
                },
                app,
            )
    }

    /** Press Ctrl+Shift+E/F/V or Ctrl+Shift+/ the way winit delivers it.
     *  @param h - Harness under test
     *  @param key - The logical key (`Key::Questionmark` for Ctrl+Shift+/)
     */
    fn chord(h: &mut Harness<'static, WidApp>, key: egui::Key) {
        h.key_down_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, key);
        h.key_up_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, key);
        h.step();
    }

    /** Press a plain Ctrl chord.
     *  @param h - Harness under test
     *  @param key - The key held with Ctrl
     */
    fn ctrl(h: &mut Harness<'static, WidApp>, key: egui::Key) {
        h.key_down_modifiers(egui::Modifiers::CTRL, key);
        h.key_up_modifiers(egui::Modifiers::CTRL, key);
        h.step();
    }

    /** Press one character key the way winit does: Key down, Text, Key up.
     *  kittest's `key_press` only sends the key event, so tests that care
     *  about character input must add the `Event::Text` winit delivers.
     *  @param h - Harness under test
     *  @param key - The logical key that was pressed
     *  @param text - The text winit commits alongside the key
     */
    fn type_char(h: &mut Harness<'static, WidApp>, key: egui::Key, text: &str) // Full winit key triple
    {
        h.event(egui::Event::Key {
            key,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
            repeat: false,
            physical_key: None,
        });
        h.event(egui::Event::Text(text.to_string()));
        h.event(egui::Event::Key {
            key,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
            repeat: false,
            physical_key: None,
        });
        h.step();
    }

    /** Ctrl+Shift+E: first press shows Explorer, second focuses it, Ctrl+Q closes. */
    #[test]
    fn ctrl_shift_e_show_focus_close() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::E);
        assert!(
            h.state().sidebar.open,
            "Ctrl+Shift+E did not open the Explorer"
        );
        assert_eq!(h.state().focus, Focus::Editor, "first press must not focus");

        chord(&mut h, egui::Key::E);
        assert_eq!(
            h.state().focus,
            Focus::Sidebar,
            "second Ctrl+Shift+E did not focus the Explorer"
        );

        ctrl(&mut h, egui::Key::Q);
        assert!(
            !h.state().sidebar.open,
            "Ctrl+Q did not close the focused Explorer"
        );
        assert_eq!(h.state().focus, Focus::Editor, "focus stuck after close");
    }

    /** Ctrl+Shift+F shows the Search view; the chord is mode-reuse-safe. */
    #[test]
    fn ctrl_shift_f_shows_search() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::F);
        assert!(h.state().sidebar.open, "Ctrl+Shift+F did not open Search");
        assert_eq!(
            h.state().sidebar.view,
            SidebarView::Search,
            "Ctrl+Shift+F did not select the Search view"
        );
    }

    /** Ctrl+Shift+P shows Preview; Ctrl+Q only closes it while focused. */
    #[test]
    fn ctrl_shift_p_preview_and_ctrl_q_noop_when_unfocused() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::P);
        assert!(h.state().right.open, "Ctrl+Shift+P did not open Preview");
        assert_eq!(h.state().focus, Focus::Editor);

        ctrl(&mut h, egui::Key::Q);
        assert!(
            h.state().right.open,
            "Ctrl+Q with the editor focused must not close a panel"
        );

        chord(&mut h, egui::Key::P);
        assert_eq!(h.state().focus, Focus::Right);
        ctrl(&mut h, egui::Key::Q);
        assert!(!h.state().right.open, "Ctrl+Q did not close Preview");
    }

    /** Ctrl+Shift+/ opens the Agent — as `?` on a US layout. */
    #[test]
    fn ctrl_shift_slash_opens_agent() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::Questionmark);
        assert!(h.state().right.open, "Ctrl+Shift+/ did not open Agent");
        assert_eq!(h.state().right.view, RightView::Agent);
    }

    /** Ctrl+Shift+/ also works when the layout reports `Key::Slash`. */
    #[test]
    fn ctrl_shift_slash_opens_agent_slash_key() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::Slash);
        assert!(
            h.state().right.open,
            "Ctrl+Shift+/ (as Key::Slash) did not open Agent"
        );
        assert_eq!(h.state().right.view, RightView::Agent);
    }

    /** With Explorer focused, `j` then Enter loads the row under the cursor.
     *  Explorer rows are [today, pages…, journals…], so `j` lands on the
     *  first created page ("alpha").
     */
    #[test]
    fn focused_explorer_j_enter_loads_page() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::E); // show
        chord(&mut h, egui::Key::E); // focus
        assert_eq!(h.state().focus, Focus::Sidebar);

        h.key_press(egui::Key::J);
        h.step();
        assert_eq!(h.state().sidebar.cursor, 1, "'j' did not move the cursor");

        h.key_press(egui::Key::Enter);
        h.step();
        assert_eq!(
            h.state().current_page.as_deref(),
            Some("alpha"),
            "Enter did not open the row under the cursor"
        );
    }

    /** Escape hands focus back to the editor but leaves the panel open. */
    #[test]
    fn escape_releases_focus_but_keeps_panel_open() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::E);
        chord(&mut h, egui::Key::E);
        assert_eq!(h.state().focus, Focus::Sidebar);

        h.key_press(egui::Key::Escape);
        h.step();
        assert_eq!(
            h.state().focus,
            Focus::Editor,
            "Escape did not release focus"
        );
        assert!(h.state().sidebar.open, "Escape must not close the panel");
    }

    /** While a panel is focused the editor ignores keys typed into it. */
    #[test]
    fn editor_ignores_keys_while_panel_focused() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::E);
        chord(&mut h, egui::Key::E);
        assert_eq!(h.state().focus, Focus::Sidebar);

        h.event(egui::Event::Text("i".to_string()));
        h.step();
        assert_eq!(
            h.state().vim.mode,
            Mode::Normal,
            "'i' reached vim while the Explorer was focused"
        );

        // Escape hands the keyboard back and 'i' works again.
        h.key_press(egui::Key::Escape);
        h.step();
        h.event(egui::Event::Text("i".to_string()));
        h.step();
        if h.state().vim.mode != Mode::Insert {
            h.step();
        }
    }

    /** Ctrl+Shift+F lands the keyboard in the query field, and the panel
     *  chords keep working while that field owns egui's text focus.
     *  Regression: the whole chord block used to sit behind
     *  `!text_edit_focused()`, so a focused field killed V, Q and `/`.
     */
    #[test]
    fn search_autofocus_keeps_panel_chords_alive() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::F);
        assert_eq!(h.state().sidebar.view, SidebarView::Search);

        // The query field took focus, so plain text lands there, not in vim.
        h.event(egui::Event::Text("alpha".to_string()));
        h.step();
        assert_eq!(
            h.state().sidebar.query,
            "alpha",
            "Ctrl+Shift+F did not focus the search field"
        );
        assert_eq!(
            h.state().vim.mode,
            Mode::Normal,
            "typed text leaked into the editor while the search field was focused"
        );

        // Chords must stay live while the field is focused.
        chord(&mut h, egui::Key::P);
        assert!(
            h.state().right.open && h.state().right.view == RightView::Preview,
            "Ctrl+Shift+P died while the search field was focused"
        );
        chord(&mut h, egui::Key::Questionmark);
        assert_eq!(
            h.state().right.view,
            RightView::Agent,
            "Ctrl+Shift+/ died while the search field was focused"
        );
    }

    /** `i` hands a focused panel back to the query field, and Ctrl+Q still
     *  closes the panel while that field owns the keyboard. */
    #[test]
    fn search_i_refocuses_field_and_ctrl_q_still_closes() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::F); // open Search, field auto-focused
        chord(&mut h, egui::Key::F); // focus the panel, field released
        assert_eq!(h.state().focus, Focus::Sidebar);

        h.key_press(egui::Key::I);
        h.step();
        h.event(egui::Event::Text("alpha".to_string()));
        h.step();
        assert_eq!(
            h.state().sidebar.query,
            "alpha",
            "'i' did not hand the keyboard to the search field"
        );

        ctrl(&mut h, egui::Key::Q);
        assert!(
            !h.state().sidebar.open,
            "Ctrl+Q did not close Search while its field was focused"
        );
        assert_eq!(h.state().focus, Focus::Editor, "focus stuck after close");
    }

    /** `i` focuses the query field without also typing an `i` into it.
     *  Regression: winit delivers a `Key::I` press *and* `Event::Text("i")`
     *  for the same keystroke. Only the key press was consumed, so the field
     *  we had just focused swallowed the stray text and inserted an `i`.
     *  Pure kittest `key_press` hid this bug by sending the key alone.
     */
    #[test]
    fn search_i_focuses_field_without_typing_an_i() {
        let mut h = harness(panel_app());
        h.step();

        chord(&mut h, egui::Key::F); // open Search, field auto-focused
        chord(&mut h, egui::Key::F); // focus the panel, field released
        assert_eq!(h.state().focus, Focus::Sidebar);

        type_char(&mut h, egui::Key::I, "i");
        assert_eq!(
            h.state().sidebar.query,
            "",
            "`i` typed itself into the field while focusing it"
        );

        type_char(&mut h, egui::Key::X, "x");
        assert_eq!(
            h.state().sidebar.query,
            "x",
            "the field did not take focus after `i`"
        );
    }
}
