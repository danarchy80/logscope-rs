//! The LogScope GUI application state and `eframe::App` implementation.

use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::collections::BTreeSet;

use egui::{Color32, ScrollArea, Spinner, Ui};

use crate::core::workspace::Workspace;
use crate::core::ingest::ingest;
use crate::core::export::{format_entries, export_entries};
use crate::core::filter::{filter_entries_with_options, FilterOptions};
use crate::core::sanitize::SanitizeConfig;
use crate::models::{Level, LogSource, LogEntry};
use crate::datetime::parse_datetime;

/// Max characters of the exported file shown in the preview panel.
const PREVIEW_LIMIT: usize = 10_000;

fn truncate_preview(text: String) -> String {
    if text.len() > PREVIEW_LIMIT {
        let mut head: String = text.chars().take(PREVIEW_LIMIT).collect();
        head.push_str("\n\n… (truncated)");
        head
    } else {
        text
    }
}

/// Level toggles in display order (most severe first).
const LEVEL_ORDER: [Level; 6] = [
    Level::Critical,
    Level::Error,
    Level::Warning,
    Level::Info,
    Level::Debug,
    Level::Trace,
];

enum WorkerMsg {
    /// One entry per ingested path: (display path, result).
    Done(Vec<(String, Result<Vec<LogSource>, String>)>),
}

pub struct LogScopeApp {
    start: String,
    end: String,
    output_path: String,

    selected_levels: BTreeSet<Level>,
    sources_filter: String,
    search: String,

    /// Set while a background pipeline thread is running.
    running: bool,
    /// Receiver for the worker thread's result (None when idle).
    rx: Option<Receiver<WorkerMsg>>,

    sanitize: SanitizeConfig,
    sanitize_text: String,
    sanitize_accounts: String,

    buckets: String,
    heatmap: Option<crate::core::heatmap::Heatmap>,

    /// Latest status line (success or error) shown under the Export button.
    status: String,
    /// Some(msg) when the last operation failed — rendered in red.
    error: Option<String>,
    /// Contents of the last successful export (truncated for preview).
    preview: Option<String>,

    /// Ordered inputs and their parsed sources. Single source of truth.
    workspace: Workspace,
    /// Cached flattened view of `workspace.merged_sources()`; rebuilt on add/remove.
    all_sources: Vec<LogSource>,
    filtered: Vec<LogEntry>,
    last_key: String,
}

impl Default for LogScopeApp {
    fn default() -> Self {
        Self {
            // Empty start/end = unbounded (far-past / far-future). Defaulting to
            // "yesterday→now" silently hid older logs on load, which read as
            // broken filtering.
            start: String::new(),
            end: String::new(),
            output_path: "unified.log".to_string(),
            sanitize: SanitizeConfig::default(),
            sanitize_text: String::new(),
            sanitize_accounts: String::new(),
            buckets: "60".to_string(),
            heatmap: None,
            selected_levels: BTreeSet::new(),
            sources_filter: String::new(),
            search: String::new(),
            running: false,
            rx: None,
            status: String::new(),
            error: None,
            preview: None,
            workspace: Workspace::new(),
            all_sources: Vec::new(),
            filtered: Vec::new(),
            last_key: String::new(),
        }
    }
}

impl LogScopeApp {
    /// Rebuild `all_sources` from the workspace.
    fn rebuild_merged(&mut self) {
        self.all_sources = self.workspace.merged_sources();
    }

    /// Spawn one background thread that ingests every path in `paths` (sequentially)
    /// and reports per-path results. Dedupes against existing workspace paths.
    fn queue_ingest(&mut self, paths: Vec<PathBuf>) {
        if self.running {
            return;
        }
        // Drop paths already in the workspace (exact trimmed-string match).
        let mut to_ingest: Vec<PathBuf> = Vec::new();
        for p in paths {
            let disp = p.display().to_string();
            if !self.workspace.contains(disp.trim()) && !to_ingest.iter().any(|q| q.display().to_string() == disp) {
                to_ingest.push(p);
            }
        }
        if to_ingest.is_empty() {
            return;
        }

        self.error = None;
        self.running = true;
        self.status = format!("Loading {} input(s)…", to_ingest.len());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let results: Vec<(String, Result<Vec<LogSource>, String>)> = to_ingest
                .into_iter()
                .map(|p| {
                    let disp = p.display().to_string();
                    let r = catch_unwind(move || ingest(&p).map_err(|e| e.to_string()));
                    let inner = match r {
                        Ok(i) => i,
                        Err(_) => Err("pipeline panicked".to_string()),
                    };
                    (disp, inner)
                })
                .collect();
            let _ = tx.send(WorkerMsg::Done(results));
        });
    }

    /// rfd multi-file pick (files + archives; a .zip/tar is just a file).
    fn add_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            self.queue_ingest(paths);
        }
    }

    fn add_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.queue_ingest(vec![path]);
        }
    }

    /// Remove a workspace item by index and re-filter.
    fn remove_item(&mut self, index: usize) {
        if self.workspace.remove(index).is_some() {
            self.rebuild_merged();
            if self.workspace.is_empty() {
                self.preview = None;
                self.heatmap = None;
                self.status.clear();
                self.last_key.clear();
            }
            self.apply_filters(true);
        }
    }

    fn clear_workspace(&mut self) {
        self.workspace.clear();
        self.all_sources.clear();
        self.preview = None;
        self.heatmap = None;
        self.status.clear();
        self.last_key.clear();
    }

    fn apply_filters(&mut self, write_file: bool) {
        self.error = None;

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

        let levels = if self.selected_levels.is_empty() {
            None
        } else {
            Some(self.selected_levels.iter().copied().collect::<Vec<_>>())
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

        let opts = FilterOptions {
            start,
            end,
            levels,
            sources,
            search,
        };

        // Parse sanitize fields
        self.sanitize.system_names = self.sanitize_text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.sanitize.account_names = self.sanitize_accounts
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        self.filtered = filter_entries_with_options(&self.all_sources, &opts);
        
        // Apply sanitization if active
        if self.sanitize.is_active() {
            crate::core::sanitize::sanitize_entries(&mut self.filtered, &self.sanitize);
        }

        if self.filtered.is_empty() {
            self.preview = Some(String::new());
        } else {
            self.preview = Some(truncate_preview(format_entries(&self.filtered)));
        }

        self.heatmap = Some(crate::core::heatmap::build_heatmap(&self.filtered, buckets));
        
        let total_entries: usize = self.all_sources.iter().map(|s| s.entries.len()).sum();
        self.status = format!(
            "{} of {} entries from {} source(s)",
            self.filtered.len(),
            total_entries,
            self.all_sources.len()
        );

        if write_file {
            let output = PathBuf::from(self.output_path.trim());
            let output = if output.as_os_str().is_empty() {
                PathBuf::from("unified.log")
            } else {
                output
            };
            if let Err(e) = export_entries(&self.filtered, &output) {
                self.error = Some(format!("Failed to write output: {}", e));
            }
        }
    }

    /// Poll the worker channel once per frame; never blocks.
    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.rx.as_ref() else { return };
        if let Ok(WorkerMsg::Done(results)) = rx.try_recv() {
            self.running = false;
            self.rx = None;
            let mut errors: Vec<String> = Vec::new();
            for (path, res) in results {
                match res {
                    Ok(sources) => self.workspace.add(path, sources),
                    Err(msg) => errors.push(format!("{path}: {msg}")),
                }
            }
            self.rebuild_merged();
            self.apply_filters(true);
            if !errors.is_empty() {
                self.error = Some(errors.join("; "));
            }
            ctx.request_repaint();
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

    fn render_heatmap(&mut self, ui: &mut Ui, hm: &crate::core::heatmap::Heatmap) {
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
    }

    fn controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.running, egui::Button::new("Add Files…"))
                .on_hover_text("Add one or more log files, .evtx, or .zip/.tar archives. Multi-select with Ctrl/Shift-click.")
                .clicked() {
                self.add_files();
            }
            if ui.add_enabled(!self.running, egui::Button::new("Add Folder…"))
                .on_hover_text("Add a folder — every log file inside is parsed recursively.")
                .clicked() {
                self.add_folder();
            }
            if ui.add_enabled(!self.running, egui::Button::new("Clear"))
                .on_hover_text("Remove all inputs from the workspace.")
                .clicked() {
                self.clear_workspace();
            }
        });
        ui.label(egui::RichText::new("Files, .zip/.tar archives, and folders are all supported.")
            .color(Color32::from_gray(140)));

        let mut remove_idx = None;
        if self.workspace.is_empty() {
            ui.label(egui::RichText::new("Workspace is empty — add files or a folder.")
                .color(Color32::from_gray(140)));
        } else {
            for idx in 0..self.workspace.len() {
                let item = &self.workspace.items()[idx];
                let path = item.path.clone();
                let sources_len = item.sources.len();
                let total: usize = item.sources.iter().map(|s| s.entries.len()).sum();
                ui.horizontal(|ui| {
                    let label = format!("{}  ({} sources, {} entries)", path, sources_len, total);
                    ui.label(label);
                    ui.add_enabled_ui(!self.running, |ui| {
                        if ui.small_button("✖ Remove")
                            .on_hover_text("Remove this input from the workspace.")
                            .clicked() {
                            remove_idx = Some(idx);
                        }
                    });
                });
            }
        }
        if let Some(idx) = remove_idx {
            self.remove_item(idx);
        }
        ui.horizontal(|ui| {
            ui.label("Start:");
            ui.add(
                egui::TextEdit::singleline(&mut self.start)
                    .hint_text("YYYY-MM-DD HH:MM:SS"),
            )
            .on_hover_text("Start of the time window (inclusive). Blank = include all earlier logs.");
            ui.label("End:");
            ui.add(
                egui::TextEdit::singleline(&mut self.end)
                    .hint_text("YYYY-MM-DD HH:MM:SS"),
            )
            .on_hover_text("End of the time window (inclusive). Blank = include all later logs.");
        });
        ui.horizontal(|ui| {
            ui.label("Output:");
            ui.text_edit_singleline(&mut self.output_path)
                .on_hover_text("Path of the unified log file written on Export.");
            if ui.button("Save…")
                .on_hover_text("Choose where the unified log file is written.")
                .clicked() {
                self.browse_save();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Sources (comma):");
            ui.text_edit_singleline(&mut self.sources_filter)
                .on_hover_text("Only show these source names, comma-separated (case-insensitive). Blank = all sources.");
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search)
                .on_hover_text("Case-insensitive substring match on the raw log message");
            ui.label("Buckets:");
            ui.text_edit_singleline(&mut self.buckets)
                .on_hover_text("Number of time columns in the heatmap.");
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Levels (click):").color(Color32::from_gray(200)));
            for lvl in LEVEL_ORDER.iter() {
                let color = self.hex_color(crate::core::heatmap::level_color(*lvl));
                let label = match lvl {
                    Level::Critical => "Critical",
                    Level::Error => "Error",
                    Level::Warning => "Warning",
                    Level::Info => "Info",
                    Level::Debug => "Debug",
                    Level::Trace => "Trace",
                    Level::Unknown => "",
                };
                let selected = self.selected_levels.contains(lvl);
                if ui.selectable_label(selected, egui::RichText::new(format!("◼ {label}")).color(color))
                    .on_hover_text(format!("Include/exclude {label}-level entries."))
                    .clicked() {
                    if selected {
                        self.selected_levels.remove(lvl);
                    } else {
                        self.selected_levels.insert(*lvl);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.sanitize.redact_ips, "Redact IPs")
                .on_hover_text("Replace IPv4 and IPv6 addresses with [REDACTED_IP].");
            ui.label("Systems (comma):");
            ui.text_edit_singleline(&mut self.sanitize_text)
                .on_hover_text("Comma-separated system/host names to redact (case-insensitive).");
            ui.label("Accounts (comma):");
            ui.text_edit_singleline(&mut self.sanitize_accounts)
                .on_hover_text("Comma-separated account/user names to redact (case-insensitive).");
        });
        ui.horizontal(|ui| {
            let export_btn = crate::gui::theme::hero_button(ui, "Export", !self.running)
                .on_hover_text("Write the filtered, time-synchronized logs to the output file.");
            if export_btn.clicked() {
                self.apply_filters(true);
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

        if !self.all_sources.is_empty() {
            let key = format!("{}|{}|{}|{}|{:?}|{}",
                self.start, self.end, self.sources_filter, self.search,
                self.selected_levels, self.buckets);
            if key != self.last_key {
                self.last_key = key;
                self.error = None;
                self.apply_filters(false);
                ctx.request_repaint();
            }
        }

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
                    let heatmap = self.heatmap.take();
                    if let Some(hm) = &heatmap {
                        if hm.sources.is_empty() {
                            ui.label(egui::RichText::new("no events").color(Color32::from_gray(156)));
                        } else {
                            self.render_heatmap(ui, hm);
                        }
                    }
                    self.heatmap = heatmap;
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
