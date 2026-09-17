//! The LogScope GUI application state and `eframe::App` implementation.

use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use chrono::{Duration, Local};
use egui::{Color32, ScrollArea, Spinner, Ui};

use crate::core::pipeline::{run_pipeline, PipelineResult};
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
enum WorkerMsg {
    Done(Result<PipelineResult, String>),
}

pub struct LogScopeApp {
    input_path: String,
    start: String,
    end: String,
    output_path: String,

    /// Set while a background pipeline thread is running.
    running: bool,
    /// Receiver for the worker thread's result (None when idle).
    rx: Option<Receiver<WorkerMsg>>,

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

        let (tx, rx) = mpsc::channel();
        self.running = true;
        self.rx = Some(rx);
        self.status = "Running pipeline…".to_string();

        thread::spawn(move || {
            // catch_unwind keeps the UI alive even if the pipeline panics.
            let result = catch_unwind(move || {
                run_pipeline(&input, start, end, &output).map_err(|e| e.to_string())
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
                    self.status = format!(
                        "Done — {} of {} entries from {} source(s) exported.",
                        res.filtered_entries, res.total_entries, res.sources
                    );
                    self.error = None;
                    self.load_preview();
                }
                Err(msg) => {
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
