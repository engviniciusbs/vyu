#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use vyu::app;

fn main() -> eframe::Result<()> {
    let _log_guard = vyu::logging::init(&vyu::config::store::config_dir().join("logs"));
    tracing::info!("vyu {} iniciando", env!("CARGO_PKG_VERSION"));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("vyu")
            // Wayland/X11 app id: matches vyu.desktop so the dock shows the right icon.
            .with_app_id("vyu")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([480.0, 270.0]),
        ..Default::default()
    };
    eframe::run_native("vyu", options, Box::new(|cc| Ok(Box::new(app::VyuApp::new(cc)))))
}
