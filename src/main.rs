mod ai;
mod app;
mod command_palette;
mod editor;
mod platform;
mod preview;
mod session;
mod settings;
mod sidebar;
mod theme;
mod vim;
mod widget;
mod workspace;

use ai::types::ChatMessage;
use eframe::egui;
use settings::Settings;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Search upward from `start` for a directory containing `workspace.toml`.
/// Returns the workspace root path if found.
fn find_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("workspace.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Load workspace settings if running inside a workspace, otherwise defaults.
fn load_cli_settings() -> Settings {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match find_workspace_root(&cwd) {
        Some(root) => {
            let settings_path = root.join("settings.toml");
            if settings_path.exists() {
                eprintln!("[config] Using workspace: {}", root.display());
                Settings::load(&settings_path)
            } else {
                eprintln!(
                    "[config] Workspace found at {} (no settings.toml, using defaults)",
                    root.display()
                );
                Settings::default()
            }
        }
        None => {
            eprintln!("[config] No workspace found, using defaults");
            Settings::default()
        }
    }
}

/// Run a search query and optionally pipe the results through an Ollama model.
fn cli_search(args: &[String]) {
    let pos = args.iter().position(|a| a == "--search").unwrap();
    let model_pos = args.iter().position(|a| a == "--model");

    let query: String = args[pos + 1..]
        .iter()
        .take_while(|a| *a != "--model")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    if query.is_empty() {
        eprintln!("Usage: wid --search <query> [--model <name>]");
        std::process::exit(1);
    }

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let cfg = load_cli_settings().ai;
    let searcher = ai::search::provider_from_config(&cfg);

    if !searcher.is_configured() {
        eprintln!("ERROR: provider '{}' is not configured.", searcher.name());
        eprintln!("  Set it in workspace settings.toml [ai] section.");
        std::process::exit(1);
    }

    eprintln!("Provider: {}", searcher.display_name());
    let results = match rt.block_on(searcher.search(&query)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Search failed: {e:?}");
            std::process::exit(1);
        }
    };

    println!("[results] Found {} results:", results.len());
    for (i, r) in results.iter().enumerate() {
        println!("  {}. {} — {}", i + 1, r.title, r.url);
    }
    println!();

    if let Some(mp) = model_pos {
        let model = args
            .get(mp + 1)
            .cloned()
            .unwrap_or_else(|| "llama3.1:8b".into());
        let context = ai::search::format_results_as_context(&query, &results);

        let client = ai::ollama::OllamaClient::new(&cfg.ollama_url);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let history = vec![ChatMessage {
            role: "user".to_string(),
            content: context,
        }];

        eprintln!("[llm] Model: {model}");
        eprintln!("[llm] Sending to {}…", cfg.ollama_url);
        let cancel = Arc::new(AtomicBool::new(false));
        rt.spawn(async move {
            client.chat_stream(model, history, tx, cancel).await;
        });

        while let Some(event) = rt.block_on(rx.recv()) {
            match event {
                ai::types::AsyncEvent::StreamChunk(chunk) => {
                    print!("{chunk}");
                }
                ai::types::AsyncEvent::StreamDone => {
                    break;
                }
                ai::types::AsyncEvent::StreamError(msg) => {
                    eprintln!("\n[llm] Error: {msg}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }
        std::io::Write::flush(&mut std::io::stdout()).ok();
        println!();
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--search") {
        cli_search(&args);
        return Ok(());
    }

    env_logger::init();
    let ws = Settings::default();

    // Restore last workspace if available
    let last_workspace = session::load_last_workspace();

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([ws.window.width, ws.window.height])
            .with_min_inner_size([ws.window.min_width, ws.window.min_height])
            .with_title("Wid"),
        ..Default::default()
    };

    eframe::run_native(
        "Wid",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(app::WidApp::new(last_workspace, ws)))
        }),
    )
}
