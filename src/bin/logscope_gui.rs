//! `logscope-gui` — windowed desktop front-end for the LogScope pipeline.

#![windows_subsystem = "windows"]

use eframe::egui;

use logscope::gui::app::LogScopeApp;

/// Decode the bundled PNG master into `egui::IconData` for the window icon.
fn window_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../../assets/logscope_icon.png"))
        .map(|img| img.to_rgba8())
        .expect("bundled icon must decode");
    let (width, height) = (img.width(), img.height());
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LogScope — Unified Log Exporter")
            .with_inner_size([720.0, 560.0])
            .with_icon(std::sync::Arc::new(window_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "LogScope — Unified Log Exporter",
        options,
        Box::new(|_cc| Ok(Box::new(LogScopeApp::default()))),
    )
}
