use crate::ai;
use crate::ai::ollama::OllamaClient;
use crate::ai::search::{format_results_as_context, provider_from_config, SearchProvider};
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

enum AppScreen {
    Welcome,
    WorkspaceReady,
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
    /// Command palette / page finder overlay (`:`, `Ctrl+P`, `Ctrl+Shift+P`).
    palette: CommandPalette,
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
        if let Some(ref ws) = self.workspace {
            match ai::persistence::load(&ws.root) {
                Ok(convs) => {
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
                Err(_) => {}
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
                if self.page_states.len() >= Self::MAX_PAGE_STATES {
                    if let Some(oldest) = self.page_state_order.pop_front() {
                        self.page_states.remove(&oldest);
                    }
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
        if let Some(ref ws) = self.workspace {
            if let Some(ref name) = self.current_page {
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
        if let Some(ref mut ws) = self.workspace {
            if let Some(ref name) = self.current_page.clone() {
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
        // Toggle the agent chat in the right side panel
        if ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::A)
        }) {
            self.right.toggle(RightView::Agent);
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

            // Palette triggers (roadmap Phase 4 step 7).
            if !self.palette.visible {
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::P)) {
                    self.refresh_palette_pages();
                    self.palette.open_pages();
                }
                if ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::P)
                }) {
                    self.palette.open_commands("");
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
                        if let Some(dir) = rfd::FileDialog::new()
                            .set_title("Select workspace directory")
                            .pick_folder()
                        {
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
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Select workspace directory")
                    .pick_folder()
                {
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
                            if let Some(dir) = rfd::FileDialog::new()
                                .set_title("Select workspace directory")
                                .pick_folder()
                            {
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
        Panel::bottom("status_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(ref ws) = self.workspace {
                    ui.label(
                        egui::RichText::new(format!("Workspace: {}", ws.config.name))
                            .color(Color32::GRAY),
                    );
                    ui.separator();
                }
                ui.label(egui::RichText::new(&self.status_message).color(Color32::GRAY));
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
                    Mode::Normal => self.accent(Color32::YELLOW),
                    Mode::Insert => self.accent(Color32::LIGHT_GREEN),
                    Mode::Visual(_) => self.accent(Color32::from_rgb(200, 150, 255)),
                    Mode::CommandLine => self.accent(Color32::LIGHT_BLUE),
                };
                let modified = if self.vim.needs_save(&self.buffer) {
                    "  ●"
                } else {
                    ""
                };

                egui::Frame::new()
                    .fill(self.theme.status.background)
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
                        .fill(self.accent(Color32::from_rgb(20, 20, 30)))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let cmd = self.vim.command_line.clone();
                                ui.label(
                                    egui::RichText::new(&cmd).color(self.accent(Color32::WHITE)),
                                );
                            });
                        });
                    ui.separator();
                }

                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .show(ui, |ui| {
                        editor_ui(
                            ui,
                            &mut self.vim,
                            &mut self.buffer,
                            &self.settings,
                            &self.theme,
                        );
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
    }

    /// Explorer view: today's journal, then pages, then past journals.
    fn render_explorer(&mut self, ui: &mut egui::Ui) {
        sidebar::panel_header(ui, "Explorer", None, &self.theme);
        ui.add_space(8.0);

        if sidebar::new_page_control(ui, &mut self.new_page_name, &self.theme) {
            self.create_page();
        }
        ui.add_space(4.0);

        // Clone the names up front so the closure can call &mut self methods.
        let (today, pages, journals) = self
            .workspace
            .as_ref()
            .map(|ws| {
                (
                    ws.today_journal(),
                    ws.pages().to_vec(),
                    ws.journals().to_vec(),
                )
            })
            .unwrap_or_default();

        if self.workspace.is_none() {
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Today's journal leads the list as a single row, not a section.
            let is_current = self.current_page.as_deref() == Some(today.as_str());
            if sidebar::nav_row(ui, &format!("Journal: {}", today), is_current, &self.theme) {
                self.load_page(&today);
            }

            sidebar::section_label(ui, "Pages", &self.theme);
            for page in &pages {
                if *page == today {
                    continue;
                }
                let selected = self.current_page.as_deref() == Some(page.as_str());
                if sidebar::nav_row(ui, &format!("{}.md", page), selected, &self.theme) {
                    let name = page.clone();
                    self.load_page(&name);
                }
            }

            sidebar::section_label(ui, "Journals", &self.theme);
            for j in &journals {
                if *j == today {
                    continue;
                }
                let selected = self.current_page.as_deref() == Some(j.as_str());
                if sidebar::nav_row(ui, j, selected, &self.theme) {
                    let name = j.clone();
                    self.load_page(&name);
                }
            }
        });
    }

    /// Search view: case-insensitive substring match across pages and journals.
    fn render_search(&mut self, ui: &mut egui::Ui) {
        sidebar::panel_header(ui, "Search", None, &self.theme);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            let width = ui.available_width();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.sidebar.query)
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
        let mut open: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for hit in &hits {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("{}:{}", hit.page, hit.line_no))
                        .color(Color32::GRAY)
                        .small(),
                );
                let text = hit.line_text.trim().to_string();
                if sidebar::nav_row(ui, &text, false, &self.theme) {
                    open = Some(hit.page.clone());
                }
            }
        });

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
                    if !self.ai_loaded_from_disk && self.ai_conversations.is_empty() {
                        if let Some(model) = models.first() {
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
                    }
                    self.ai_status = format!("Ready — {} models", self.ai_models.len());
                }
                AsyncEvent::ModelsError(msg) => {
                    if !self.ai_loaded_from_disk {
                        self.ai_status = format!("Ollama: {msg}");
                    }
                }
                AsyncEvent::StreamChunk(content) => {
                    if let Some(cid) = self.ai_stream_conv_id {
                        if let Some(conv) = self.ai_conversations.iter_mut().find(|c| c.id == cid) {
                            if let Some(msg) = conv.messages.last_mut() {
                                if msg.role == "assistant" {
                                    msg.content.push_str(&content);
                                }
                            }
                        }
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
                    if let Some(cid) = self.ai_stream_conv_id.take() {
                        if let Some(conv) = self.ai_conversations.iter_mut().find(|c| c.id == cid) {
                            conv.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("Error: {msg}"),
                            });
                        }
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
        if notes_on {
            if let Some(ref ws) = self.workspace {
                let ctx = ai::context::build_context(&ws.root, self.settings.ai.max_notes);
                if !ctx.is_empty() {
                    history.insert(
                        0,
                        ChatMessage {
                            role: "system".to_string(),
                            content: format!(
                                "Here are the user's recent notes for context.\n{}",
                                ctx
                            ),
                        },
                    );
                }
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
        if let Some(conv) = self.current_conv_mut() {
            if conv.messages.last().map(|m| m.role.as_str()) == Some("system") {
                conv.messages.pop();
            }
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

        let dialog = rfd::FileDialog::new()
            .set_title("Export Conversation")
            .add_filter("Markdown", &["md"])
            .add_filter("JSON", &["json"])
            .set_file_name(&default_name);

        if let Some(path) = dialog.save_file() {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("md");
            let result = match ext {
                "json" => serde_json::to_string_pretty(conv)
                    .map_err(std::io::Error::other)
                    .and_then(|json| std::fs::write(&path, json)),
                _ => {
                    let mut md = format!("# AI Conversation — {}\n\n", conv.model);
                    for msg in &conv.messages {
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
    fn render_preview_panel(&mut self, ui: &mut egui::Ui) {
        let hint = self.current_page.as_ref().map(|name| format!("{name}.md"));
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
        egui::ScrollArea::vertical()
            .id_salt("preview_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_markdown(
                    ui,
                    &self.buffer.text,
                    self.is_dark,
                    &self.theme.invert_excludes,
                );
            });
    }

    // ── AI Panel Rendering ─────────────────────────────────

    fn render_ai_panel(&mut self, ui: &mut egui::Ui) {
        sidebar::panel_header(
            ui,
            RightView::Agent.label(),
            Some("Ctrl+Shift+A"),
            &self.theme,
        );

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
            if let Some(conv) = self.current_conv() {
                if !conv.messages.is_empty() {
                    ui.add_space(4.0);
                    if ui.button(RichText::new("↓ Export").size(12.0)).clicked() {
                        self.ai_export_conversation();
                    }
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
        let scroll = ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(self.ai_auto_scroll);

        let mut retry = false;
        scroll.show(ui, |ui| {
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

            if self.ai_is_streaming {
                if let Some(last) = conv.messages.last() {
                    if last.role == "assistant" && last.content.is_empty() {
                        let dots = self.ai_loading_dots();
                        ui.colored_label(
                            Color32::GRAY,
                            RichText::new(format!("Generating{dots}")).italics(),
                        );
                    }
                }
            }
        });

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
                                .find(|(v, _)| *v == &sel)
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
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

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
        let mut h = harness(agent_panel_app());

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
     */
    #[test]
    fn after_click_arrow_keys_navigate_and_text_edits() {
        let mut h = harness(editor_app());
        h.step();
        click_editor(&mut h);

        // Symptom (a): cannot navigate — ArrowDown must move cursor 0 → line 2.
        h.key_press(egui::Key::ArrowDown);
        h.step();
        assert_eq!(
            h.state().buffer.cursor,
            6,
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
}
