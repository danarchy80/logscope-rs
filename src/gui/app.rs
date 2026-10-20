//! The LogScope GUI application state and `eframe::App` implementation.
//!
//! Layout (design "Command Rail & Right Inspector"):
//! - icon rail (far left): add files/folder, clear, theme cycle
//! - workspace sidebar: ingested inputs as cards, click to focus a card's sources
//! - top bar: time window, search, source filter, severity chips, buckets, Export
//! - center: time heatmap over a virtualized unified-log table
//! - right inspector: selected entry details + sanitize controls
//! - status bar: counts, theme, encoding

use std::collections::BTreeSet;
use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Panel, Painter,
    Pos2, Rect, RichText, ScrollArea, Sense, Stroke, CentralPanel, Vec2,
};

use crate::core::workspace::Workspace;
use crate::core::ingest::ingest;
use crate::core::filter::{filter_entries_with_options, FilterOptions};
use crate::core::llm::{self, AnalysisResult, KeyEvent, LlmConfig};
use crate::core::sanitize::SanitizeConfig;
use crate::models::{Level, LogSource, LogEntry};
use crate::datetime::parse_datetime;
use crate::gui::theme::{self, ThemeId};

/// Level toggles in display order (most severe first).
const LEVEL_ORDER: [Level; 6] = [
    Level::Critical,
    Level::Error,
    Level::Warning,
    Level::Info,
    Level::Debug,
    Level::Trace,
];

/// Row height of the virtualized log table.
const ROW_H: f32 = 20.0;
/// Approximate advance width of one monospace char at the table font size.
const CHAR_W: f32 = 6.9;
/// Column x-offsets inside a log row.
const COL_TS: f32 = 8.0;
const COL_LEVEL: f32 = 150.0;
const COL_SOURCE: f32 = 232.0;
const COL_MSG: f32 = 392.0;

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn short_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

enum WorkerMsg {
    /// One entry per ingested path: (display path, result).
    Done(Vec<(String, Result<Vec<LogSource>, String>)>),
    /// Result of a background LLM analysis request.
    LlmDone(Result<AnalysisResult, String>),
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

    buckets: usize,
    heatmap: Option<crate::core::heatmap::Heatmap>,

    /// Latest status line (success or error) shown in the status bar.
    status: String,
    /// Some(msg) when the last operation failed — rendered in the error color.
    error: Option<String>,

    /// Ordered inputs and their parsed sources. Single source of truth.
    workspace: Workspace,
    /// Cached flattened view of `workspace.merged_sources()`; rebuilt on add/remove.
    all_sources: Vec<LogSource>,
    filtered: Vec<LogEntry>,
    last_key: String,

    // --- UI state (redesign) ---
    pub theme: ThemeId,
    /// Workspace card the user clicked to focus its sources (None = all).
    selected_item: Option<usize>,
    /// Index into `filtered` of the row shown in the inspector.
    selected_entry: Option<usize>,
    /// Per-level counts of the current `filtered` set (LEVEL_ORDER order).
    level_counts: Vec<(Level, usize)>,

    // --- LLM analysis ---
    pub llm_cfg: LlmConfig,
    /// Whether the OS keyring is reachable (set at startup by the bin).
    pub keyring_available: bool,
    /// Apply the sanitize pipeline before sending entries to the endpoint.
    pub redact_before_send: bool,
    analysis: Option<AnalysisResult>,
    analysis_status: String,
    analysis_running: bool,
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
            buckets: 60,
            heatmap: None,
            selected_levels: BTreeSet::new(),
            sources_filter: String::new(),
            search: String::new(),
            running: false,
            rx: None,
            status: String::new(),
            error: None,
            workspace: Workspace::new(),
            all_sources: Vec::new(),
            filtered: Vec::new(),
            last_key: String::new(),
            theme: ThemeId::default(),
            selected_item: None,
            selected_entry: None,
            level_counts: Vec::new(),
            llm_cfg: LlmConfig::default(),
            keyring_available: true,
            redact_before_send: true,
            analysis: None,
            analysis_status: String::new(),
            analysis_running: false,
        }
    }
}

impl LogScopeApp {
    fn pal(&self) -> &'static theme::Palette {
        theme::palette(self.theme)
    }

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
            if self.selected_item == Some(index) {
                self.selected_item = None;
            } else if let Some(si) = self.selected_item {
                if si > index {
                    self.selected_item = Some(si - 1);
                }
            }
            self.rebuild_merged();
            if self.workspace.is_empty() {
                self.heatmap = None;
                self.status.clear();
                self.last_key.clear();
                self.selected_entry = None;
            }
            self.apply_filters(true);
        }
    }

    fn clear_workspace(&mut self) {
        self.workspace.clear();
        self.all_sources.clear();
        self.heatmap = None;
        self.status.clear();
        self.last_key.clear();
        self.selected_item = None;
        self.selected_entry = None;
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

        // A focused workspace card overrides the free-text source filter.
        let sources: Option<Vec<String>> = match self.selected_item.and_then(|i| self.workspace.items().get(i)) {
            Some(item) => {
                let names: Vec<String> = item.sources.iter().map(|s| s.name.clone()).collect();
                if names.is_empty() { None } else { Some(names) }
            }
            None => {
                let parsed: Vec<String> = self.sources_filter
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if parsed.is_empty() { None } else { Some(parsed) }
            }
        };

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

        let opts = FilterOptions {
            start,
            end,
            levels,
            sources,
            search,
        };

        self.filtered = filter_entries_with_options(&self.all_sources, &opts);

        // Apply sanitization if active
        if self.sanitize.is_active() {
            crate::core::sanitize::sanitize_entries(&mut self.filtered, &self.sanitize);
        }

        // Per-level counts of the visible set (for the severity chips).
        let mut counts: Vec<(Level, usize)> = LEVEL_ORDER.iter().map(|l| (*l, 0usize)).collect();
        for e in &self.filtered {
            if let Some(c) = counts.iter_mut().find(|(l, _)| *l == e.level) {
                c.1 += 1;
            }
        }
        self.level_counts = counts;

        self.heatmap = Some(crate::core::heatmap::build_heatmap(&self.filtered, self.buckets));

        let total_entries: usize = self.all_sources.iter().map(|s| s.entries.len()).sum();
        self.status = format!(
            "{} of {} entries · {} source(s)",
            self.filtered.len(),
            total_entries,
            self.all_sources.len()
        );

        if write_file {
            use crate::core::export::export_entries;
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

    /// Synchronously load one log file and attach a canned analysis so the
    /// timeline can be exercised without a live endpoint (screenshots/demos).
    /// Activated via `LOGSCOPE_DEMO=/path/to/log`.
    pub fn load_demo(&mut self, path: &std::path::Path) {
        match crate::core::ingest::ingest(path) {
            Ok(sources) => {
                self.workspace.add(path.display().to_string(), sources);
                self.rebuild_merged();
                self.apply_filters(true);
            }
            Err(e) => self.error = Some(format!("demo ingest: {e}")),
        }
        let ev = |ts: &str, level: &str, title: &str, detail: &str, source: &str| KeyEvent {
            timestamp: ts.to_string(),
            level: level.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            source: source.to_string(),
        };
        self.analysis = Some(AnalysisResult {
            summary: "Demo analysis: escalating failures between 08:53 and 08:57 \
                      (thermal warning → pool exhaustion → unhandled exception), \
                      recovered after an automatic restart."
                .to_string(),
            key_events: vec![
                ev("2025-10-09 08:53:20", "INFO", "SSH login accepted", "publickey auth from 203.0.113.9", "syslog"),
                ev("2025-10-09 08:54:20", "WARNING", "CPU temperature above threshold", "kernel reports 85C", "syslog"),
                ev("2025-10-09 08:55:20", "ERROR", "Database connection pool exhausted", "30s timeout waiting for a free connection", "app"),
                ev("2025-10-09 08:56:20", "CRITICAL", "Unhandled exception in request handler", "service restarting", "app"),
                ev("2025-10-09 08:57:20", "INFO", "Service restarted successfully", "listening on :8080", "app"),
            ],
            recommendations: vec![
                "Add thermal alerting at 80C before the kernel threshold.".to_string(),
                "Raise the DB pool ceiling or add backpressure on the request path.".to_string(),
            ],
        });
        self.analysis_status = "Demo analysis loaded".to_string();
    }

    /// Poll the worker channel once per frame; never blocks.
    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.rx.as_ref() else { return };
        match rx.try_recv() {
            Ok(WorkerMsg::Done(results)) => {
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
                self.selected_entry = None;
                self.apply_filters(true);
                if !errors.is_empty() {
                    self.error = Some(errors.join("; "));
                }
                ctx.request_repaint();
            }
            Ok(WorkerMsg::LlmDone(result)) => {
                self.analysis_running = false;
                self.rx = None;
                match result {
                    Ok(a) => {
                        let n = a.key_events.len();
                        self.analysis_status =
                            format!("Analysis complete — {n} key event(s)");
                        self.analysis = Some(a);
                    }
                    Err(e) => {
                        self.analysis_status = "Analysis failed".to_string();
                        self.error = Some(format!("LLM: {e}"));
                    }
                }
                ctx.request_repaint();
            }
            Err(_) => {}
        }
    }

    fn browse_save(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            self.output_path = path.display().to_string();
        }
    }

    /// Send the (optionally redacted) filtered entries to the configured
    /// OpenAI-compatible endpoint on a worker thread.
    fn start_analysis(&mut self) {
        if self.analysis_running || self.running || self.filtered.is_empty() {
            return;
        }
        self.analysis_running = true;
        self.analysis_status = "Sending logs to LLM…".to_string();
        self.error = None;

        // Redaction path: `filtered` is already sanitized in place when the
        // sanitize pipeline is active; this clone re-applies it for the case
        // where it is off but the user still asked to redact (no-op then —
        // the UI warns about that explicitly).
        let mut entries = self.filtered.clone();
        if self.redact_before_send {
            crate::core::sanitize::sanitize_entries(&mut entries, &self.sanitize);
        }
        let cfg = self.llm_cfg.clone();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let r = catch_unwind(move || llm::analyze(&entries, &cfg));
            let msg = match r {
                Ok(Ok(a)) => WorkerMsg::LlmDone(Ok(a)),
                Ok(Err(e)) => WorkerMsg::LlmDone(Err(e)),
                Err(_) => WorkerMsg::LlmDone(Err("analysis panicked".to_string())),
            };
            let _ = tx.send(msg);
        });
    }

    // ------------------------------------------------------------------
    // Heatmap
    // ------------------------------------------------------------------

    fn render_heatmap(&self, ui: &mut egui::Ui, hm: &crate::core::heatmap::Heatmap) {
        let pal = self.pal();
        let cell_w = 6.0;
        let row_h = 14.0;
        let label_w = 140.0;
        let bottom = 18.0;

        let grid_w = hm.num_buckets as f32 * cell_w;
        let grid_h = hm.sources.len() as f32 * row_h;

        let w = label_w + grid_w + 8.0;
        let h = grid_h + bottom + 4.0;

        let (rect, _response) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let painter = ui.painter_at(rect);

        let origin_x = rect.min.x;
        let margin_top = rect.min.y;

        for (i, source) in hm.sources.iter().enumerate() {
            let row_center_y = margin_top + i as f32 * row_h + row_h / 2.0;
            painter.text(
                Pos2::new(origin_x + label_w - 6.0, row_center_y),
                Align2::RIGHT_CENTER,
                truncate(source, 22),
                FontId::proportional(11.0),
                pal.text_muted,
            );

            for j in 0..hm.num_buckets {
                let lvl = hm.cells[i][j];
                if lvl != Level::Unknown {
                    let color = pal.level_color(lvl);
                    let cell = Rect::from_min_size(
                        Pos2::new(origin_x + label_w + j as f32 * cell_w, margin_top + i as f32 * row_h),
                        Vec2::new(cell_w - 1.0, row_h - 1.0),
                    );
                    painter.rect_filled(cell, 0.0, color);
                }
            }
        }

        if let (Some(min_ts), Some(max_ts)) = (hm.min_ts, hm.max_ts) {
            let mid_ts = min_ts
                + chrono::Duration::milliseconds(
                    max_ts.signed_duration_since(min_ts).num_milliseconds() / 2,
                );
            let format_ts = |ts: chrono::DateTime<chrono::Utc>| ts.format("%m-%d %H:%M").to_string();

            let axis_y = margin_top + grid_h + 8.0;
            let start_x = origin_x + label_w;
            painter.text(
                Pos2::new(start_x, axis_y),
                Align2::LEFT_TOP,
                format_ts(min_ts),
                FontId::proportional(9.0),
                pal.text_muted,
            );
            painter.text(
                Pos2::new(start_x + grid_w / 2.0, axis_y),
                Align2::CENTER_TOP,
                format_ts(mid_ts),
                FontId::proportional(9.0),
                pal.text_muted,
            );
            painter.text(
                Pos2::new(start_x + grid_w, axis_y),
                Align2::RIGHT_TOP,
                format_ts(max_ts),
                FontId::proportional(9.0),
                pal.text_muted,
            );
        }
    }

    fn render_heatmap_legend(&self, ui: &mut egui::Ui) {
        let pal = self.pal();
        ui.horizontal(|ui| {
            for lvl in LEVEL_ORDER.iter() {
                let color = pal.level_color(*lvl);
                ui.label(RichText::new("■").size(10.0).color(color));
                ui.label(RichText::new(lvl.as_str()).size(9.5).color(pal.text_muted));
                ui.add_space(2.0);
            }
        });
    }

    // ------------------------------------------------------------------
    // LLM key-event timeline
    // ------------------------------------------------------------------

    /// Map a free-form level string from the LLM onto our Level enum.
    fn level_from_str(s: &str) -> Level {
        match s.trim().to_ascii_uppercase().as_str() {
            "CRITICAL" | "FATAL" => Level::Critical,
            "ERROR" | "ERR" => Level::Error,
            "WARNING" | "WARN" => Level::Warning,
            "INFO" => Level::Info,
            "DEBUG" => Level::Debug,
            "TRACE" => Level::Trace,
            _ => Level::Unknown,
        }
    }

    /// Draw the LLM's key events on a horizontal time axis. Returns the index
    /// into `filtered` of the entry nearest to a clicked event dot.
    fn render_event_timeline(&self, ui: &mut egui::Ui) -> Option<usize> {
        let pal = self.pal();
        let events = self.analysis.as_ref()?.key_events.as_slice();
        if events.is_empty() {
            return None;
        }

        let (min_ts, max_ts) = self.filtered.iter().fold(
            (
                None::<chrono::DateTime<chrono::Utc>>,
                None::<chrono::DateTime<chrono::Utc>>,
            ),
            |(mn, mx), e| {
                (
                    Some(mn.map_or(e.timestamp, |m| m.min(e.timestamp))),
                    Some(mx.map_or(e.timestamp, |m| m.max(e.timestamp))),
                )
            },
        );
        let (min_ts, max_ts) = (min_ts?, max_ts?);
        let span_ms = (max_ts - min_ts).num_milliseconds().max(1) as f64;

        let width = ui.available_width().max(140.0);
        let h = 52.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
        let painter = ui.painter_at(rect);
        let axis_y = rect.min.y + 24.0;
        let x0 = rect.min.x + 10.0;
        let w = (width - 20.0).max(10.0);
        painter.line_segment(
            [Pos2::new(x0, axis_y), Pos2::new(x0 + w, axis_y)],
            Stroke::new(1.5, pal.border),
        );

        let mut clicked: Option<usize> = None;
        for (i, ev) in events.iter().enumerate() {
            // Position by parsed timestamp when possible; even spacing otherwise.
            let frac = match parse_datetime(ev.timestamp.trim()) {
                Ok(ts) => (((ts - min_ts).num_milliseconds() as f64 / span_ms).clamp(0.0, 1.0)) as f32,
                Err(_) => (i as f32 + 0.5) / events.len() as f32,
            };
            let cx = x0 + frac * w;
            let color = pal.level_color(Self::level_from_str(&ev.level));
            let center = Pos2::new(cx, axis_y);
            painter.circle_filled(center, 4.5, color);
            painter.circle_stroke(center, 4.5, Stroke::new(1.0, pal.canvas));
            let tip = format!(
                "{} [{}] {} — {}",
                ev.timestamp, ev.level, ev.title, ev.detail
            );
            let hit = Rect::from_center_size(center, Vec2::new(14.0, 14.0));
            let resp = ui
                .interact(hit, ui.id().with(("evt", i)), Sense::click())
                .on_hover_text(tip);
            if resp.clicked() {
                if let Ok(ts) = parse_datetime(ev.timestamp.trim()) {
                    clicked = self
                        .filtered
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, e)| (e.timestamp - ts).num_milliseconds().abs())
                        .map(|(i, _)| i);
                }
            }
        }

        painter.text(
            Pos2::new(x0, rect.max.y - 14.0),
            Align2::LEFT_TOP,
            min_ts.format("%m-%d %H:%M").to_string(),
            FontId::proportional(9.0),
            pal.text_muted,
        );
        painter.text(
            Pos2::new(x0 + w, rect.max.y - 14.0),
            Align2::RIGHT_TOP,
            max_ts.format("%m-%d %H:%M").to_string(),
            FontId::proportional(9.0),
            pal.text_muted,
        );
        clicked
    }

    // ------------------------------------------------------------------
    // Log table (virtualized)
    // ------------------------------------------------------------------

    fn render_log_row(
        &self,
        ui: &mut egui::Ui,
        idx: usize,
        entry: &LogEntry,
        selected: bool,
    ) -> bool {
        let pal = self.pal();
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());
        let painter = ui.painter_at(rect);

        if selected {
            painter.rect_filled(rect, 0.0, pal.selection_fill);
            painter.rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(2.0, ROW_H)),
                0.0,
                pal.accent,
            );
        } else if resp.hovered() {
            painter.rect_filled(rect, 0.0, pal.hover_fill);
        }

        let cy = rect.center().y;
        let mono = FontId::monospace(11.5);
        painter.text(
            Pos2::new(rect.min.x + COL_TS, cy),
            Align2::LEFT_CENTER,
            entry.timestamp.format("%m-%d %H:%M:%S%.3f").to_string(),
            mono.clone(),
            pal.text_muted,
        );
        painter.text(
            Pos2::new(rect.min.x + COL_LEVEL, cy),
            Align2::LEFT_CENTER,
            entry.level.as_str(),
            mono.clone(),
            pal.level_color(entry.level),
        );
        painter.text(
            Pos2::new(rect.min.x + COL_SOURCE, cy),
            Align2::LEFT_CENTER,
            truncate(&short_name(&entry.source), 22),
            mono.clone(),
            pal.kicker,
        );
        let msg = entry.raw_lines.first().map(String::as_str).unwrap_or("");
        let max_chars = (((rect.width() - COL_MSG - 8.0) / CHAR_W).floor() as usize).max(8);
        painter.text(
            Pos2::new(rect.min.x + COL_MSG, cy),
            Align2::LEFT_CENTER,
            truncate(msg, max_chars),
            mono,
            pal.text,
        );

        let _ = idx;
        resp.clicked()
    }

    fn render_log_table(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let total = self.filtered.len();

        // Column header (fixed, outside the scroll area).
        let header_rect = ui.allocate_rect(
            Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 18.0)),
            Sense::hover(),
        ).rect;
        let hp: Painter = ui.painter_at(header_rect);
        let hy = header_rect.center().y;
        let hfont = FontId::proportional(10.0);
        for (x, label) in [
            (COL_TS, "TIMESTAMP"),
            (COL_LEVEL, "LEVEL"),
            (COL_SOURCE, "SOURCE"),
            (COL_MSG, "MESSAGE"),
        ] {
            hp.text(
                Pos2::new(header_rect.min.x + x, hy),
                Align2::LEFT_CENTER,
                label,
                hfont.clone(),
                pal.text_muted,
            );
        }
        hp.line_segment(
            [
                Pos2::new(header_rect.min.x, header_rect.max.y),
                Pos2::new(header_rect.max.x, header_rect.max.y),
            ],
            Stroke::new(1.0, pal.border),
        );

        if total == 0 {
            ui.add_space(12.0);
            ui.label(
                RichText::new("No entries match the current filters.")
                    .size(12.0)
                    .color(pal.text_muted),
            );
            return;
        }

        let mut clicked: Option<usize> = None;
        let selected = self.selected_entry;
        let filtered = &self.filtered;
        ScrollArea::vertical()
            .id_salt("logtable")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let first = ((viewport.min.y / ROW_H).max(0.0) as usize)
                    .saturating_sub(4)
                    .min(total);
                let count = (viewport.height() / ROW_H).ceil() as usize + 8;
                let last = (first + count).min(total);
                ui.add_space(first as f32 * ROW_H);
                for idx in first..last {
                    let entry = &filtered[idx];
                    if self.render_log_row(ui, idx, entry, selected == Some(idx)) {
                        clicked = Some(idx);
                    }
                }
                ui.add_space((total - last) as f32 * ROW_H);
            });
        if let Some(i) = clicked {
            self.selected_entry = Some(i);
        }
    }

    // ------------------------------------------------------------------
    // Panels
    // ------------------------------------------------------------------

    fn rail(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let mut cycle_theme = false;
        let mut add_f = false;
        let mut add_d = false;
        let mut clear = false;
        Panel::left("rail")
            .exact_size(46.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(pal.canvas)
                    .inner_margin(Margin::same(6))
                    .stroke(Stroke::new(1.0, pal.border)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if ui.add_sized([30.0, 28.0], egui::Button::new(RichText::new("＋").size(15.0)))
                        .on_hover_text("Add log files, .evtx, or .zip/.tar archives (multi-select in dialog).")
                        .clicked()
                    {
                        add_f = true;
                    }
                    if ui.add_sized([30.0, 28.0], egui::Button::new(RichText::new("▤").size(14.0)))
                        .on_hover_text("Add a folder — every log file inside is parsed recursively.")
                        .clicked()
                    {
                        add_d = true;
                    }
                    if ui.add_sized([30.0, 28.0], egui::Button::new(RichText::new("✕").size(13.0)))
                        .on_hover_text("Remove all inputs from the workspace.")
                        .clicked()
                    {
                        clear = true;
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    if ui.add_sized(
                        [30.0, 28.0],
                        egui::Button::new(RichText::new("◐").size(15.0).color(pal.kicker)),
                    )
                    .on_hover_text(format!("Theme: {} — click to cycle.", self.theme.label()))
                    .clicked()
                    {
                        cycle_theme = true;
                    }
                });
            });
        if add_f {
            self.add_files();
        }
        if add_d {
            self.add_folder();
        }
        if clear {
            self.clear_workspace();
        }
        if cycle_theme {
            self.theme = self.theme.next();
            theme::install(ui.ctx(), self.theme);
            ui.ctx().request_repaint();
        }
    }

    fn workspace_panel(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        Panel::left("workspace")
            .default_size(252.0)
            .frame(
                Frame::new()
                    .fill(pal.panel)
                    .inner_margin(Margin { left: 10, right: 10, top: 10, bottom: 10 })
                    .stroke(Stroke::new(1.0, pal.border)),
            )
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("WORKSPACE · {}", self.workspace.len()))
                        .size(10.5)
                        .strong()
                        .color(pal.kicker),
                );
                ui.add_space(4.0);
                ScrollArea::vertical()
                    .id_salt("wslist")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.workspace.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Empty — use ＋ in the rail to add files, archives, or a folder.")
                                    .size(11.5)
                                    .color(pal.text_muted),
                            );
                        }
                        let mut remove_idx: Option<usize> = None;
                        let mut toggle_idx: Option<usize> = None;
                        for idx in 0..self.workspace.len() {
                            let item = &self.workspace.items()[idx];
                            let path = item.path.clone();
                            let sources_len = item.sources.len();
                            let total: usize = item.sources.iter().map(|s| s.entries.len()).sum();
                            let selected = self.selected_item == Some(idx);
                            let frame = Frame::new()
                                .fill(if selected { pal.hover } else { pal.raised })
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::symmetric(8, 6))
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected { pal.accent } else { pal.border },
                                ))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(truncate(&short_name(&path), 26))
                                                .size(12.0),
                                        );
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if ui
                                                .small_button("✕")
                                                .on_hover_text("Remove this input.")
                                                .clicked()
                                            {
                                                remove_idx = Some(idx);
                                            }
                                        });
                                    });
                                    ui.label(
                                        RichText::new(format!("{sources_len} sources · {total} entries"))
                                            .size(10.5)
                                            .monospace()
                                            .color(pal.text_muted),
                                    );
                                });
                            let resp = ui.interact(
                                frame.response.rect,
                                ui.id().with(("wsitem", idx)),
                                Sense::click(),
                            );
                            if resp.clicked() && remove_idx.is_none() {
                                toggle_idx = Some(idx);
                            }
                        }
                        if let Some(idx) = remove_idx {
                            self.remove_item(idx);
                        } else if let Some(idx) = toggle_idx {
                            self.selected_item = if self.selected_item == Some(idx) {
                                None
                            } else {
                                Some(idx)
                            };
                            self.selected_entry = None;
                            self.apply_filters(false);
                        }
                    });
            });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        Panel::top("header")
            .frame(
                Frame::new()
                    .fill(pal.panel)
                    .inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 8 })
                    .stroke(Stroke::new(1.0, pal.border)),
            )
            .show(ui, |ui| {
                // Row 1: time window, search, sources, output, export.
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Start").size(11.0).color(pal.text_muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.start)
                            .hint_text("YYYY-MM-DD HH:MM:SS")
                            .desired_width(150.0),
                    )
                    .on_hover_text("Start of the time window (inclusive). Blank = include all earlier logs.");
                    ui.label(RichText::new("→").color(pal.text_muted));
                    ui.label(RichText::new("End").size(11.0).color(pal.text_muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.end)
                            .hint_text("YYYY-MM-DD HH:MM:SS")
                            .desired_width(150.0),
                    )
                    .on_hover_text("End of the time window (inclusive). Blank = include all later logs.");
                    ui.separator();
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search messages…")
                            .desired_width(160.0),
                    )
                    .on_hover_text("Case-insensitive substring match on the raw log message.");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sources_filter)
                            .hint_text("Sources (comma)")
                            .desired_width(130.0),
                    )
                    .on_hover_text("Only show these source names, comma-separated (case-insensitive). Blank = all sources.");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::hero_button(ui, pal, "Export", !self.running)
                            .on_hover_text("Write the filtered, time-synchronized logs to the output file.")
                            .clicked()
                        {
                            self.apply_filters(true);
                        }
                        if self.running {
                            ui.add(egui::Spinner::new());
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.output_path)
                                .hint_text("unified.log")
                                .desired_width(150.0),
                        )
                        .on_hover_text("Path of the unified log file written on Export.");
                        if ui.small_button("Save…")
                            .on_hover_text("Choose where the unified log file is written.")
                            .clicked()
                        {
                            self.browse_save();
                        }
                    });
                });
                ui.add_space(4.0);
                // Row 2: severity chips + buckets + counts.
                ui.horizontal(|ui| {
                    for lvl in LEVEL_ORDER.iter() {
                        let count = self
                            .level_counts
                            .iter()
                            .find(|(l, _)| l == lvl)
                            .map(|(_, c)| *c)
                            .unwrap_or(0);
                        let active =
                            self.selected_levels.is_empty() || self.selected_levels.contains(lvl);
                        let label = format!("● {} {}", lvl.as_str(), count);
                        let color = pal.level_color(*lvl);
                        let text = if active {
                            RichText::new(label).size(11.0).color(color)
                        } else {
                            RichText::new(label).size(11.0).color(pal.text_muted)
                        };
                        if ui
                            .selectable_label(active, text)
                            .on_hover_text(format!("Include/exclude {}-level entries.", lvl.as_str()))
                            .clicked()
                        {
                            if self.selected_levels.contains(lvl) {
                                self.selected_levels.remove(lvl);
                            } else {
                                self.selected_levels.insert(*lvl);
                            }
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(err) = &self.error {
                            ui.label(RichText::new(err).size(11.0).color(pal.error));
                        } else if !self.status.is_empty() {
                            ui.label(
                                RichText::new(&self.status)
                                    .size(11.0)
                                    .monospace()
                                    .color(pal.kicker),
                            );
                        }
                        ui.separator();
                        ui.label(RichText::new("Buckets").size(11.0).color(pal.text_muted));
                        ui.add(egui::DragValue::new(&mut self.buckets).range(10..=500));
                    });
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let pal = self.pal();
        Panel::bottom("status")
            .frame(
                Frame::new()
                    .fill(pal.panel)
                    .inner_margin(Margin { left: 10, right: 10, top: 3, bottom: 3 })
                    .stroke(Stroke::new(1.0, pal.border)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let dot = if self.error.is_some() { pal.error } else { pal.accent };
                    ui.label(RichText::new("●").size(9.0).color(dot));
                    ui.label(
                        RichText::new(if self.running { "Working…" } else { "Ready" })
                            .size(10.5)
                            .color(pal.text_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("UTF-8 · LF")
                                .size(10.5)
                                .monospace()
                                .color(pal.text_muted),
                        );
                        ui.separator();
                        ui.label(
                            RichText::new(self.theme.label())
                                .size(10.5)
                                .color(pal.text_muted),
                        );
                    });
                });
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        Panel::right("inspector")
            .default_size(300.0)
            .frame(
                Frame::new()
                    .fill(pal.panel)
                    .inner_margin(Margin { left: 10, right: 10, top: 10, bottom: 10 })
                    .stroke(Stroke::new(1.0, pal.border)),
            )
            .show(ui, |ui| {
                ui.label(
                    RichText::new("INSPECTOR")
                        .size(10.5)
                        .strong()
                        .color(pal.kicker),
                );
                ui.add_space(4.0);
                let entry = self
                    .selected_entry
                    .and_then(|i| self.filtered.get(i))
                    .cloned();
                match entry {
                    Some(e) => {
                        ScrollArea::vertical()
                            .id_salt("inspect")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let kv = |ui: &mut egui::Ui, k: &str, v: String, c: Color32| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(k).size(10.0).color(pal.text_muted),
                                        );
                                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                            ui.label(RichText::new(v).size(11.0).monospace().color(c));
                                        });
                                    });
                                };
                                kv(ui, "Time", e.timestamp.format("%Y-%m-%d %H:%M:%S%.3f %Z").to_string(), pal.text);
                                kv(ui, "Level", e.level.as_str().to_string(), pal.level_color(e.level));
                                kv(ui, "Source", e.source.clone(), pal.kicker);
                                kv(ui, "Line", (e.original_line_number + 1).to_string(), pal.text);
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("RAW")
                                        .size(10.0)
                                        .strong()
                                        .color(pal.text_muted),
                                );
                                let raw = e.raw_lines.join("\n");
                                ui.label(
                                    RichText::new(raw)
                                        .size(10.5)
                                        .monospace()
                                        .color(pal.text),
                                );
                            });
                    }
                    None => {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Select a row in the unified log to inspect it.")
                                .size(11.5)
                                .color(pal.text_muted),
                        );
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label(
                    RichText::new("SANITIZE")
                        .size(10.5)
                        .strong()
                        .color(pal.kicker),
                );
                ui.checkbox(&mut self.sanitize.redact_ips, "Redact IPs")
                    .on_hover_text("Replace IPv4 and IPv6 addresses with [REDACTED_IP].");
                ui.label(RichText::new("Systems (comma)").size(10.5).color(pal.text_muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.sanitize_text)
                        .hint_text("HOST-07, SRV-DC01")
                        .desired_width(f32::INFINITY),
                );
                ui.label(RichText::new("Accounts (comma)").size(10.5).color(pal.text_muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.sanitize_accounts)
                        .hint_text("svc-backup")
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(10.0);
                ui.separator();
                ui.label(
                    RichText::new("AI ANALYSIS")
                        .size(10.5)
                        .strong()
                        .color(pal.kicker),
                );
                ui.label(RichText::new("Base URL").size(10.5).color(pal.text_muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.llm_cfg.base_url)
                        .hint_text("http://192.168.50.7:8731/v1")
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("OpenAI-compatible endpoint base (…/v1). Works with local llama.cpp/Halogen servers and cloud providers.");
                ui.label(RichText::new("Model").size(10.5).color(pal.text_muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.llm_cfg.model)
                        .hint_text("halogen-qwen3.8-flash-next")
                        .desired_width(f32::INFINITY),
                );
                ui.label(RichText::new("API key (optional)").size(10.5).color(pal.text_muted));
                ui.add(
                    egui::TextEdit::singleline(&mut self.llm_cfg.api_key)
                        .password(true)
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("Sent as a Bearer token. Stored in the OS keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service) — never in the settings file. Leave empty for local servers.");
                if !self.keyring_available && !self.llm_cfg.api_key.is_empty() {
                    ui.label(
                        RichText::new("⚠ OS keyring unavailable — this key stays in memory only and will not be saved.")
                            .size(10.0)
                            .color(pal.error),
                    );
                }
                ui.checkbox(&mut self.redact_before_send, "Redact before send")
                    .on_hover_text("Apply the sanitize pipeline above to the payload. Keep it on for cloud providers; uncheck only for trusted local endpoints.");
                if self.redact_before_send && !self.sanitize.is_active() {
                    ui.label(
                        RichText::new("⚠ Sanitize is off — logs will be sent unredacted.")
                            .size(10.0)
                            .color(pal.error),
                    );
                }
                ui.add_space(4.0);
                let can_analyze = !self.analysis_running
                    && !self.running
                    && !self.filtered.is_empty()
                    && !self.llm_cfg.base_url.trim().is_empty()
                    && !self.llm_cfg.model.trim().is_empty();
                ui.horizontal(|ui| {
                    if theme::hero_button(ui, pal, "Analyze", can_analyze)
                        .on_hover_text(if self.filtered.is_empty() {
                            "Load and filter logs first."
                        } else {
                            "Send the filtered logs to the endpoint and render a key-event timeline."
                        })
                        .clicked()
                    {
                        self.start_analysis();
                    }
                    if self.analysis_running {
                        ui.add(egui::Spinner::new().size(13.0));
                    }
                });
                if !self.analysis_status.is_empty() {
                    ui.label(
                        RichText::new(&self.analysis_status)
                            .size(10.5)
                            .monospace()
                            .color(pal.text_muted),
                    );
                }
            });
    }

    fn center(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(pal.canvas)
                    .inner_margin(Margin::same(8)),
            )
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // Heatmap panel (natural height).
                    let heatmap = self.heatmap.take();
                    Frame::new()
                        .fill(pal.panel)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(8))
                        .stroke(Stroke::new(1.0, pal.border))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("TIME HEATMAP")
                                        .size(10.5)
                                        .strong()
                                        .color(pal.kicker),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    self.render_heatmap_legend(ui);
                                });
                            });
                            match &heatmap {
                                Some(hm) if !hm.sources.is_empty() => {
                                    ScrollArea::horizontal()
                                        .id_salt("hmscroll")
                                        .show(ui, |ui| {
                                            self.render_heatmap(ui, hm);
                                        });
                                }
                                _ => {
                                    ui.label(
                                        RichText::new("no events")
                                            .size(11.0)
                                            .color(pal.text_muted),
                                    );
                                }
                            }
                        });
                    self.heatmap = heatmap;
                    ui.add_space(6.0);
                    // AI event timeline + findings (visible after an analysis).
                    if self.analysis.is_some() {
                        Frame::new()
                            .fill(pal.panel)
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::same(8))
                            .stroke(Stroke::new(1.0, pal.border))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("AI EVENT TIMELINE")
                                            .size(10.5)
                                            .strong()
                                            .color(pal.kicker),
                                    );
                                    if let Some(a) = &self.analysis {
                                        ui.label(
                                            RichText::new(format!("{} key event(s)", a.key_events.len()))
                                                .size(10.0)
                                                .color(pal.text_muted),
                                        );
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new("click a dot to jump to the log row")
                                                .size(9.5)
                                                .color(pal.text_muted),
                                        );
                                    });
                                });
                                if let Some(idx) = self.render_event_timeline(ui) {
                                    self.selected_entry = Some(idx);
                                }
                                if let Some(a) = &self.analysis {
                                    ScrollArea::vertical()
                                        .id_salt("aifindings")
                                        .max_height(150.0)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            if !a.summary.is_empty() {
                                                ui.label(
                                                    RichText::new("SUMMARY")
                                                        .size(10.0)
                                                        .strong()
                                                        .color(pal.text_muted),
                                                );
                                                ui.label(
                                                    RichText::new(&a.summary)
                                                        .size(11.5)
                                                        .color(pal.text),
                                                );
                                            }
                                            if !a.recommendations.is_empty() {
                                                ui.add_space(4.0);
                                                ui.label(
                                                    RichText::new("RECOMMENDATIONS")
                                                        .size(10.0)
                                                        .strong()
                                                        .color(pal.text_muted),
                                                );
                                                for (i, rec) in a.recommendations.iter().enumerate() {
                                                    ui.label(
                                                        RichText::new(format!("{}. {}", i + 1, rec))
                                                            .size(11.0)
                                                            .color(pal.text),
                                                    );
                                                }
                                            }
                                        });
                                }
                            });
                        ui.add_space(6.0);
                    }
                    // Unified log panel (fills remaining height).
                    Frame::new()
                        .fill(pal.panel)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin { left: 6, right: 6, top: 8, bottom: 6 })
                        .stroke(Stroke::new(1.0, pal.border))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.render_log_table(ui);
                        });
                });
            });
    }
}

impl eframe::App for LogScopeApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);

        if !self.all_sources.is_empty() {
            let key = format!(
                "{}|{}|{}|{}|{:?}|{}|{}",
                self.start,
                self.end,
                self.sources_filter,
                self.search,
                self.selected_levels,
                self.buckets,
                self.selected_item.map(|i| i.to_string()).unwrap_or_default(),
            );
            if key != self.last_key {
                self.last_key = key;
                self.error = None;
                self.selected_entry = None;
                self.apply_filters(false);
                ctx.request_repaint();
            }
        }

        if self.running {
            // Keep repainting so the spinner animates and results are picked up.
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.top_bar(ui);
        self.status_bar(ui);
        self.rail(ui);
        self.workspace_panel(ui);
        self.inspector(ui);
        self.center(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("theme", self.theme.as_str().to_string());
        storage.set_string("llm.base_url", self.llm_cfg.base_url.clone());
        storage.set_string("llm.model", self.llm_cfg.model.clone());
        // The API key lives in the OS keyring, never in this file. Best-effort
        // sync on every save; also wipes any legacy plaintext copy.
        if self.keyring_available {
            let _ = crate::core::secret::store_api_key(&self.llm_cfg.api_key);
        }
        storage.set_string("llm.api_key", String::new());
        storage.set_string("llm.redact", if self.redact_before_send { "1" } else { "0" }.to_string());
    }
}
