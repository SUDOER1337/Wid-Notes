mod app;
mod editor;
mod platform;
mod preview;
mod session;
mod settings;
mod theme;
mod vim;
mod widget;
mod workspace;

use eframe::egui;
use settings::Settings;

fn main() -> eframe::Result<()> {
    env_logger::init();
    let ws = Settings::default();

    // Restore last workspace if available
    let last_workspace = session::load_last_workspace();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([ws.window.width, ws.window.height])
            .with_min_inner_size([ws.window.min_width, ws.window.min_height])
            .with_title("Nortex"),
        ..Default::default()
    };

    eframe::run_native(
        "Nortex",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(app::NortexApp::new(last_workspace, ws)))
        }),
    )
}
