#![windows_subsystem = "windows"]/// 文件检索助手 - Main entry point

///
/// Usage:
///   文件检索助手                  (GUI mode)
///   文件检索助手 --cli [options]  (CLI/ES mode)
///   文件检索助手 --minimized      (Start minimized to tray)

mod engine;
mod types;
mod config;
mod cli;
mod file_list;
mod history;
mod rename;
mod http_server;
mod etp;
mod sdk;
mod gui;
mod tray;
mod autostart;
mod watcher;
mod ftp;

use std::sync::Arc;
use engine::Engine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let is_cli_mode = args.iter().any(|a| a == "--cli");
    let start_minimized = args.iter().any(|a| a == "--minimized" || a == "-minimized");

    // Create and initialize engine
    let engine = Arc::new(initialize_engine());

    if is_cli_mode {
        run_cli(engine, &args);
        Ok(())
    } else {
        run_gui(engine, start_minimized)?;
        Ok(())
    }
}

/// Initialize the engine with configuration and default index sources
fn initialize_engine() -> Engine {
    let engine = Engine::new();

    // Load config
    let config_path = config::Config::get_config_path();
    if let Ok(cfg) = config::Config::load(&config_path.to_string_lossy()) {
        *engine.config.write().unwrap() = cfg;
    }

    // Add default index sources
    add_default_sources(&engine);
    engine
}

/// Add default NTFS volumes as index sources
fn add_default_sources(engine: &Engine) {
    let mut sources = engine.index_sources.write().unwrap();
    if let Ok(profile) = std::env::var("USERPROFILE") {
        sources.push(types::IndexSource {
            index_type: types::IndexType::Folder,
            path: std::path::PathBuf::from(&profile),
            enabled: true,
            label: Some(profile),
        });
    }
    if let Ok(docs) = std::env::var("USERPROFILE") {
        let docs_path = format!("{}\\Documents", docs);
        if std::path::Path::new(&docs_path).exists() {
            sources.push(types::IndexSource {
                index_type: types::IndexType::Folder,
                path: std::path::PathBuf::from(&docs_path),
                enabled: true,
                label: Some(docs_path),
            });
        }
    }
}

fn run_cli(engine: Arc<Engine>, args: &[String]) {
    let engine_ref = engine.clone();
    // Build index first
    if let Err(e) = engine_ref.build_index() {
        eprintln!("Index error: {}", e);
        std::process::exit(1);
    }

    let cli_args: Vec<String> = args.iter()
        .filter(|a| *a != "--cli")
        .cloned()
        .collect();

    match cli::parse_args(&cli_args) {
        Ok((opts, _extra)) => {
            match cli::run_cli(&opts, &engine_ref) {
                Ok(code) => std::process::exit(code),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            cli::print_help();
            std::process::exit(1);
        }
    }
}

/// Run the GUI application
fn run_gui(engine: Arc<Engine>, start_minimized: bool) -> Result<(), Box<dyn std::error::Error>> {
    let app = gui::app::EverythingApp::new_with_engine(engine, start_minimized);

    // 不再在启动时启动 tray-helper，改为首次最小化到托盘时启动
    // 由 gui::app 在 close_requested / CancelClose 时调用 tray::ensure_tray_helper()

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 700.0])
        .with_min_inner_size([600.0, 400.0])
        .with_title("文件检索助手");

    // Try to load icon.ico for the window
    let icon_paths = ["icon.ico", "../icon.ico", "E:/MyEverything/everything-rs/icon.ico"];
    for p in &icon_paths {
        if let Ok(icon_data) = load_icon_from_ico(p) {
            viewport = viewport.with_icon(icon_data);
            break;
        }
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "文件检索助手",
        options,
        Box::new(|_cc| {
            Box::new(app)
        }),
    )?;

    // eframe::run_native 返回 → 主窗口已关闭 → 清理 tray-helper
    tray::kill_tray_helper();

    Ok(())
}

/// Load icon from .ico file as egui::IconData
fn load_icon_from_ico(path: &str) -> Result<egui::IconData, Box<dyn std::error::Error>> {
    let ico_data = std::fs::read(path)?;
    let icon_dir = ico::IconDir::read(std::io::Cursor::new(&ico_data))?;
    // Find the best matching icon (prefer 32x32)
    let entry = icon_dir.entries().iter()
        .min_by_key(|e| (e.width() as i32 - 32).abs() + (e.height() as i32 - 32).abs())
        .ok_or("No icon entries")?;
    let icon_img = entry.decode()?;
    let rgba = icon_img.rgba_data().to_vec();
    Ok(egui::IconData {
        rgba,
        width: icon_img.width(),
        height: icon_img.height(),
    })
}
