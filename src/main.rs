mod app;
mod editor;
mod preview;
mod settings;
mod vim;
mod widget;
mod workspace;

use eframe::egui;
use settings::Settings;

fn main() -> eframe::Result<()> {
    env_logger::init();
    let ws = Settings::default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([ws.window.width, ws.window.height])
            .with_min_inner_size([ws.window.min_width, ws.window.min_height])
            .with_title("RustNote"),
        ..Default::default()
    };

    eframe::run_native(
        "RustNote",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(app::RustNoteApp::new()))
        }),
    )
}
