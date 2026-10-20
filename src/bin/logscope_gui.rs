//! `logscope-gui` — windowed desktop front-end for the LogScope pipeline.

#![windows_subsystem = "windows"]

use eframe::egui;

use logscope::gui::app::LogScopeApp;
use logscope::gui::theme::{self, ThemeId};

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
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 680.0])
            .with_icon(std::sync::Arc::new(window_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "LogScope — Unified Log Exporter",
        options,
        Box::new(|cc| {
            let mut app = LogScopeApp::default();
            if let Some(storage) = cc.storage {
                if let Some(s) = storage.get_string("theme") {
                    if let Some(t) = ThemeId::from_str(&s) {
                        app.theme = t;
                    }
                }
                app.llm_cfg.base_url = storage.get_string("llm.base_url").unwrap_or_default();
                app.llm_cfg.model = storage.get_string("llm.model").unwrap_or_default();
                // API key: prefer the OS keyring; fall back to a legacy
                // plaintext copy in settings (migrated + wiped by save()).
                app.keyring_available = logscope::core::secret::keyring_available();
                app.llm_cfg.api_key = logscope::core::secret::load_api_key()
                    .or_else(|| storage.get_string("llm.api_key").filter(|s| !s.is_empty()))
                    .unwrap_or_default();
                // Redaction defaults to ON when nothing was stored yet.
                app.redact_before_send = storage
                    .get_string("llm.redact")
                    .map(|s| s != "0")
                    .unwrap_or(true);
            }
            if let Ok(p) = std::env::var("LOGSCOPE_DEMO") {
                app.load_demo(std::path::Path::new(&p));
            }
            theme::install(&cc.egui_ctx, app.theme);
            Ok(Box::new(app))
        }),
    )
}
