use crate::editor::Buffer;
use crate::preview::render_markdown;
use crate::settings::Settings;

use crate::vim::{CommandAction, Mode, PageState, VimState};
use crate::widget::editor_ui;
use crate::workspace::Workspace;
use eframe::egui;
use eframe::egui::panel::Panel;
use eframe::egui::Color32;
use std::collections::HashMap;

enum AppScreen {
    Welcome,
    WorkspaceReady,
}

pub struct RustNoteApp {
    screen: AppScreen,
    workspace: Option<Workspace>,
    buffer: Buffer,
    vim: VimState,
    settings: Settings,
    show_preview: bool,
    new_page_name: String,
    status_message: String,
    show_prompt: bool,
    prompt_path: String,
    prompt_name: String,
    current_page: Option<String>,
    page_states: HashMap<String, PageState>,
}

impl RustNoteApp {
    pub fn new() -> Self {
        Self {
            screen: AppScreen::Welcome,
            workspace: None,
            buffer: Buffer::new(),
            vim: VimState::new(),
            settings: Settings::default(),
            show_preview: false,
            new_page_name: String::new(),
            status_message: String::new(),
            show_prompt: true,
            prompt_path: String::new(),
            prompt_name: String::from("my-workspace"),
            current_page: None,
            page_states: HashMap::new(),
        }
    }

    fn open_workspace(&mut self, path: &std::path::Path) {
        if let Some(ws) = Workspace::open(path) {
            self.settings = ws.load_settings();
            self.workspace = Some(ws);
            self.open_today_journal();
            self.screen = AppScreen::WorkspaceReady;
            self.show_prompt = false;
            self.status_message = "Workspace opened".to_string();
        } else {
            self.status_message = "Failed to open workspace".to_string();
        }
    }

    fn create_workspace(&mut self, path: &std::path::Path, name: &str) {
        match Workspace::create(path, name) {
            Ok(ws) => {
                ws.save_settings(&self.settings).ok();
                self.settings = ws.load_settings();
                self.workspace = Some(ws);
                self.open_today_journal();
                self.screen = AppScreen::WorkspaceReady;
                self.show_prompt = false;
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
}

impl eframe::App for RustNoteApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Handle Ctrl+S
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_current_page();
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

impl RustNoteApp {
    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("RustNote");
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

impl RustNoteApp {
    fn render_workspace(&mut self, ui: &mut egui::Ui) {
        // Process deferred actions
        let cmd = std::mem::replace(&mut self.vim.command_action, CommandAction::None);
        match cmd {
            CommandAction::Save => self.save_current_page(),
            CommandAction::Quit => {
                // Save before quit if dirty (like :wq)
                if self.vim.needs_save(&self.buffer) {
                    self.save_current_page();
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            CommandAction::ForceQuit => {
                // Force quit without saving
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            CommandAction::SaveQuit => {
                self.save_current_page();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            CommandAction::SaveForceQuit => {
                self.save_current_page();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            CommandAction::Open(name) => {
                self.load_page(&name);
            }
            CommandAction::None => {}
        }

        // Menu bar
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
                    ui.checkbox(&mut self.show_preview, "Markdown Preview");
                });
                if ui.button("Today's Journal").clicked() {
                    self.open_today_journal();
                }
            });
        });

        // Sidebar
        Panel::left("sidebar")
            .resizable(true)
            .min_size(160.0)
            .show_inside(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading("Pages");
                    ui.separator();
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.new_page_name)
                                .hint_text("New page...")
                                .desired_width(ui.available_width() - 32.0),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.create_page();
                        }
                        if ui.button("+").clicked() {
                            self.create_page();
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Collect workspace data upfront to avoid borrow conflicts
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

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if self.workspace.is_some() {
                            let is_current = self.current_page.as_deref() == Some(today.as_str());
                            if ui
                                .selectable_label(is_current, format!("Journal: {}", today))
                                .clicked()
                            {
                                self.load_page(&today);
                            }

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("Pages").color(Color32::GRAY).small());
                            for page in &pages {
                                if *page == today {
                                    continue;
                                }
                                let selected = self.current_page.as_deref() == Some(page.as_str());
                                if ui
                                    .selectable_label(selected, format!("{}.md", page))
                                    .clicked()
                                {
                                    let name = page.clone();
                                    self.load_page(&name);
                                }
                            }

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("Journals").color(Color32::GRAY).small());
                            for j in &journals {
                                if *j == today {
                                    continue;
                                }
                                let selected = self.current_page.as_deref() == Some(j.as_str());
                                if ui.selectable_label(selected, j.clone()).clicked() {
                                    let name = j.clone();
                                    self.load_page(&name);
                                }
                            }
                        }
                    });
                });
            });

        // Editor + preview
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.show_preview {
                Panel::right("preview_panel")
                    .resizable(true)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.heading("Preview");
                            ui.separator();
                            ui.add_space(4.0);
                            render_markdown(ui, &self.buffer.text);
                        });
                    });
            }

            if self.current_page.is_some() || !self.buffer.text.is_empty() {
                let mode_str = match self.vim.mode {
                    Mode::Normal => "NORMAL",
                    Mode::Insert => "INSERT",
                    Mode::Visual(_) => "VISUAL",
                    Mode::CommandLine => "CMD",
                };
                let mode_color = match self.vim.mode {
                    Mode::Normal => Color32::YELLOW,
                    Mode::Insert => Color32::LIGHT_GREEN,
                    Mode::Visual(_) => Color32::from_rgb(200, 150, 255),
                    Mode::CommandLine => Color32::LIGHT_BLUE,
                };
                let modified = if self.vim.needs_save(&self.buffer) {
                    "  ●"
                } else {
                    ""
                };

                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 20, 30))
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
                        .fill(Color32::from_rgb(20, 20, 30))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let cmd = self.vim.command_line.clone();
                                ui.label(egui::RichText::new(&cmd).color(Color32::WHITE));
                            });
                        });
                    ui.separator();
                }

                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .show(ui, |ui| {
                        editor_ui(ui, &mut self.vim, &mut self.buffer, &self.settings);
                    });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("RustNote");
                    ui.label("A local-first knowledge workspace");
                    ui.add_space(10.0);
                    if ui.button("Create a new page").clicked() {
                        self.new_page_name = "untitled".to_string();
                        self.create_page();
                    }
                });
            }
        });

        // Status bar
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
    }
}
