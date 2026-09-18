//! The LogScope GUI application state and `eframe::App` implementation.

use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use chrono::{Duration, Local};
use egui::{Color32, ScrollArea, Spinner, Ui};

use crate::core::pipeline::PipelineResult;
use crate::datetime::parse_datetime;

/// Max characters of the exported file shown in the preview panel.
const PREVIEW_LIMIT: usize = 10_000;

/// Far-past default lower bound (inclusive): 1970-01-01T00:00:00Z.
fn default_start() -> String {
    let yesterday = Local::now().date_naive() - Duration::days(1);
    format!("{} 00:00:00", yesterday.format("%Y-%m-%d"))
}

/// Default upper bound: now.
fn default_end() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Message sent from the background pipeline thread back to the UI.
struct WorkerOutput {
    result: PipelineResult,
    heatmap: Option<crate::core::heatmap::Heatmap>,
}

enum WorkerMsg {
    Done(Result<WorkerOutput, String>),
}

pub struct LogScopeApp {
    input_path: String,
    start: String,
    end: String,
    output_path: String,

    levels_filter: String,
    sources_filter: String,
    search: String,

    /// Set while a background pipeline thread is running.
    running: bool,
    /// Receiver for the worker thread's result (None when idle).
    rx: Option<Receiver<WorkerMsg>>,

    buckets: String,
    heatmap: Option<crate::core::heatmap::Heatmap>,

    /// Latest status line (success or error) shown under the Export button.
    status: String,
    /// Some(msg) when the last operation failed — rendered in red.
    error: Option<String>,
    /// Contents of the last successful export (truncated for preview).
    preview: Option<String>,
}

impl Default for LogScopeApp {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            start: default_start(),
            end: default_end(),
            output_path: "unified.log".to_string(),
            buckets: "60".to_string(),
            heatmap: None,
            levels_filter: String::new(),
            sources_filter: String::new(),
            search: String::new(),
            running: false,
            rx: None,
            status: String::new(),
            error: None,
            preview: None,
        }
    }
}

impl LogScopeApp {
    /// Validate inputs and spawn the pipeline on a background thread.
    fn start_export(&mut self) {
        if self.running {
            return;
        }
        self.error = None;
        self.status.clear();
        self.preview = None;

        let input = PathBuf::from(self.input_path.trim());
        if input.as_os_str().is_empty() {
            self.error = Some("input path is empty".to_string());
            return;
        }

        // Empty/blank bounds fall back to the same far-past / far-future
        // defaults the CLI uses.
        let start = if self.start.trim().is_empty() {
            parse_datetime("1970-01-01T00:00:00Z")
        } else {
            parse_datetime(self.start.trim())
        };
        let end = if self.end.trim().is_empty() {
            parse_datetime("9999-12-31T23:59:59Z")
        } else {
            parse_datetime(self.end.trim())
        };

        let (start, end) = match (start, end) {
            (Ok(s), Ok(e)) => (s, e),
            (Err(msg), _) | (_, Err(msg)) => {
                self.error = Some(msg);
                return;
            }
        };
        if end < start {
            self.error = Some(format!("end ({end}) is before start ({start})"));
            return;
        }

        let output = PathBuf::from(self.output_path.trim());
        let output = if output.as_os_str().is_empty() {
            PathBuf::from("unified.log")
        } else {
            output
        };
        
        let mut parsed_levels = Vec::new();
        for t in self.levels_filter.split(',') {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            if let Some(lvl) = crate::models::Level::parse(t) {
                parsed_levels.push(lvl);
            } else {
                self.error = Some(format!("Unknown level: {t}"));
                return;
            }
        }
        let levels = if parsed_levels.is_empty() { None } else { Some(parsed_levels) };

        let mut parsed_sources = Vec::new();
        for t in self.sources_filter.split(',') {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            parsed_sources.push(t.to_string());
        }
        let sources = if parsed_sources.is_empty() { None } else { Some(parsed_sources) };

        let search = if self.search.trim().is_empty() {
            None
        } else {
            Some(self.search.trim().to_string())
        };

        let buckets = self.buckets.trim();
        let buckets = if buckets.is_empty() {
            60
        } else {
            match buckets.parse::<usize>() {
                Ok(b) => b,
                Err(_) => {
                    self.error = Some(format!("Invalid buckets: {}", self.buckets));
                    return;
                }
            }
        };

        let heatmap_sources = sources.clone();

        let opts = crate::core::pipeline::PipelineOptions {
            start,
            end,
            levels,
            sources,
            search,
        };

        let (tx, rx) = mpsc::channel();
        self.running = true;
        self.rx = Some(rx);
        self.status = "Running pipeline…".to_string();
        thread::spawn(move || {
            // catch_unwind keeps the UI alive even if the pipeline panics.
            let result = catch_unwind(move || {
                let res = crate::core::pipeline::run_pipeline_with_options(&[input.clone()], &opts, &output).map_err(|e| e.to_string())?;
                
                let heatmap = match crate::core::pipeline::run_heatmap_data(&[input], start, end, heatmap_sources, buckets) {
                    Ok((hm, _count)) => Some(hm),
                    Err(_) => None,
                };
                
                Ok(WorkerOutput { result: res, heatmap })
            });
            let msg = match result {
                Ok(inner) => WorkerMsg::Done(inner),
                Err(_) => WorkerMsg::Done(Err("pipeline panicked".to_string())),
            };
            let _ = tx.send(msg);
        });
    }

    /// Poll the worker channel once per frame; never blocks.
    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.rx.as_ref() else { return };
        if let Ok(WorkerMsg::Done(result)) = rx.try_recv() {
            self.running = false;
            self.rx = None;
            match result {
                Ok(res) => {
                    self.heatmap = res.heatmap;
                    self.status = format!(
                        "Done — {} of {} entries from {} source(s) exported.",
                        res.result.filtered_entries, res.result.total_entries, res.result.sources
                    );
                    self.error = None;
                    self.load_preview();
                }
                Err(msg) => {
                    self.heatmap = None;
                    self.status = "Export failed.".to_string();
                    self.error = Some(msg);
                }
            }
            ctx.request_repaint();
        }
    }

    /// Read the exported file into the preview buffer (truncated).
    fn load_preview(&mut self) {
        let path = PathBuf::from(self.output_path.trim());
        let path = if path.as_os_str().is_empty() {
            PathBuf::from("unified.log")
        } else {
            path
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                if text.len() > PREVIEW_LIMIT {
                    let mut head: String = text.chars().take(PREVIEW_LIMIT).collect();
                    head.push_str("\n\n… (truncated)");
                    self.preview = Some(head);
                } else {
                    self.preview = Some(text);
                }
            }
            Err(e) => {
                self.preview = Some(format!("(could not read output file: {e})"));
            }
        }
    }

    fn browse_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.input_path = path.display().to_string();
        }
    }

    fn browse_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.input_path = path.display().to_string();
        }
    }

    fn browse_save(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            self.output_path = path.display().to_string();
        }
    }

    fn hex_color(&self, s: &str) -> Color32 {
        if let Some(stripped) = s.strip_prefix('#') {
            if stripped.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&stripped[0..2], 16),
                    u8::from_str_radix(&stripped[2..4], 16),
                    u8::from_str_radix(&stripped[4..6], 16),
                ) {
                    return Color32::from_rgb(r, g, b);
                }
            }
        }
        Color32::GRAY
    }

    fn render_heatmap(&self, ui: &mut Ui, hm: &crate::core::heatmap::Heatmap) {
        let cell_w = 6.0;
        let row_h = 14.0;
        let label_w = 140.0;
        let bottom = 18.0;

        let grid_w = hm.num_buckets as f32 * cell_w;
        let grid_h = hm.sources.len() as f32 * row_h;
        
        let w = label_w + grid_w + 8.0;
        let h = grid_h + bottom + 4.0;

        let (rect, _response) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
        let painter = ui.painter_at(rect);

        let origin_x = rect.min.x;
        let margin_top = rect.min.y;

        for (i, source) in hm.sources.iter().enumerate() {
            let row_center_y = margin_top + i as f32 * row_h + row_h / 2.0;
            
            let display_source = if source.chars().count() > 22 {
                let mut s: String = source.chars().take(21).collect();
                s.push('…');
                s
            } else {
                source.clone()
            };

            painter.text(
                egui::pos2(origin_x + label_w - 6.0, row_center_y),
                egui::Align2::RIGHT_CENTER,
                display_source,
                egui::FontId::proportional(11.0),
                Color32::from_gray(220),
            );

            for j in 0..hm.num_buckets {
                let lvl = hm.cells[i][j];
                if lvl != crate::models::Level::Unknown {
                    let color = self.hex_color(crate::core::heatmap::level_color(lvl));
                    let x = origin_x + label_w + j as f32 * cell_w;
                    let y = margin_top + i as f32 * row_h;
                    let cell_rect = egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(cell_w - 1.0, row_h - 1.0)
                    );
                    painter.rect_filled(cell_rect, 0.0, color);
                }
            }
        }

        if let (Some(min_ts), Some(max_ts)) = (hm.min_ts, hm.max_ts) {
            let mid_ts = min_ts + chrono::Duration::milliseconds(max_ts.signed_duration_since(min_ts).num_milliseconds() / 2);
            let format_ts = |ts: chrono::DateTime<chrono::Utc>| ts.format("%Y-%m-%d %H:%M").to_string();
            
            let axis_y = margin_top + grid_h + 8.0;
            let start_x = origin_x + label_w;
            
            painter.text(
                egui::pos2(start_x, axis_y),
                egui::Align2::LEFT_TOP,
                format_ts(min_ts),
                egui::FontId::proportional(9.0),
                Color32::from_gray(156),
            );
            
            painter.text(
                egui::pos2(start_x + grid_w / 2.0, axis_y),
                egui::Align2::CENTER_TOP,
                format_ts(mid_ts),
                egui::FontId::proportional(9.0),
                Color32::from_gray(156),
            );
            
            painter.text(
                egui::pos2(start_x + grid_w, axis_y),
                egui::Align2::RIGHT_TOP,
                format_ts(max_ts),
                egui::FontId::proportional(9.0),
                Color32::from_gray(156),
            );
        }

        ui.horizontal(|ui| {
            let legend_levels = [
                crate::models::Level::Critical,
                crate::models::Level::Error,
                crate::models::Level::Warning,
                crate::models::Level::Info,
                crate::models::Level::Debug,
                crate::models::Level::Trace,
            ];
            for lvl in legend_levels.iter() {
                let color = self.hex_color(crate::core::heatmap::level_color(*lvl));
                let label = match lvl {
                    crate::models::Level::Critical => "Critical",
                    crate::models::Level::Error => "Error",
                    crate::models::Level::Warning => "Warning",
                    crate::models::Level::Info => "Info",
                    crate::models::Level::Debug => "Debug",
                    crate::models::Level::Trace => "Trace",
                    crate::models::Level::Unknown => "",
                };
                ui.colored_label(color, label);
            }
        });
    }

    fn controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Input:");
            ui.text_edit_singleline(&mut self.input_path);
            if ui.button("Browse…").clicked() {
                self.browse_file();
            }
            if ui.button("Folder…").clicked() {
                self.browse_folder();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Start:");
            ui.add(
                egui::TextEdit::singleline(&mut self.start)
                    .hint_text("YYYY-MM-DD HH:MM:SS"),
            );
            ui.label("End:");
            ui.add(
                egui::TextEdit::singleline(&mut self.end)
                    .hint_text("YYYY-MM-DD HH:MM:SS"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Output:");
            ui.text_edit_singleline(&mut self.output_path);
            if ui.button("Save…").clicked() {
                self.browse_save();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Levels (comma):");
            ui.text_edit_singleline(&mut self.levels_filter);
            ui.label("Sources (comma):");
            ui.text_edit_singleline(&mut self.sources_filter);
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search);
            ui.label("Buckets:");
            ui.text_edit_singleline(&mut self.buckets);
        });
        ui.horizontal(|ui| {
            let export_btn = crate::gui::theme::hero_button(ui, "Export", !self.running);
            if export_btn.clicked() {
                self.start_export();
            }
            if self.running {
                ui.add(Spinner::new());
            }
            if let Some(err) = &self.error {
                ui.colored_label(Color32::from_rgb(220, 80, 80), err);
            } else if !self.status.is_empty() {
                ui.label(&self.status);
            }
        });
    }
}

impl eframe::App for LogScopeApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        if self.running {
            // Keep repainting so the spinner animates and results are picked up.
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("controls").show(ui, |ui| {
            self.controls(ui);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(hm) = &self.heatmap {
                        if hm.sources.is_empty() {
                            ui.label(egui::RichText::new("no events").color(Color32::from_gray(156)));
                        } else {
                            self.render_heatmap(ui, hm);
                        }
                    }
                    if let Some(preview) = &self.preview {
                        ui.monospace(preview.as_str());
                    } else if self.running {
                        ui.label("Exporting…");
                    } else {
                        ui.label("Preview appears here after an export.");
                    }
                });
        });
    }
}
