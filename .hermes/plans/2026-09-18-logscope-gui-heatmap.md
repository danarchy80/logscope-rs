# LogScope Rust: inline egui heatmap in the GUI

Architect: high-cap model (Nous). Coder: agy (Gemini 3.1 Pro High). QA: high-cap model.

## Objective

Render the severity heatmap INLINE in the `logscope-gui` window using egui
painter primitives (colored rects + text labels). No SVG file, no new
dependency. The GUI already has search/levels/sources/time-window; only the
heatmap is missing from the GUI (it is CLI-only today).

## CRITICAL environment note

System `cargo` is apt 1.93.1 (too old for egui). Use rustup 1.98.1 for EVERY
cargo command:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version   # must print 1.98.1
```

## Current state (read first)

- `src/core/heatmap.rs` has `Heatmap { sources: Vec<String>, num_buckets:
  usize, min_ts/max_ts: Option<DateTime<Utc>>, cells: Vec<Vec<Level>> }`,
  `build_heatmap(&[LogEntry], num_buckets) -> Heatmap`, and
  `render_heatmap_svg(&Heatmap) -> String`. The level→hex color mapping is a
  PRIVATE closure inside `render_heatmap_svg`.
- `src/core/pipeline.rs` has `run_heatmap(inputs, start, end, sources,
  num_buckets, output_path) -> Result<usize, IngestError>` (writes SVG).
- `src/gui/app.rs`: `LogScopeApp` has `input_path/start/end/output_path`,
  `levels_filter/sources_filter/search` (strings), `running/rx/status/error/
  preview`. `start_export()` parses filters, spawns a thread that calls
  `run_pipeline_with_options`, sends `WorkerMsg::Done(Result<PipelineResult,
  String>)`. `poll_worker` handles it. The central panel shows the text preview.

## Changes

### 1. `src/core/heatmap.rs` — expose the color mapping

Add a public function (replace the private closure, reuse it inside
`render_heatmap_svg` so SVG output is byte-identical to before):

```rust
/// Hex color for a severity level (dark-theme palette). Unknown = grid bg.
pub fn level_color(level: Level) -> &'static str {
    match level {
        Level::Critical => "#7f1d1d",
        Level::Error => "#dc2626",
        Level::Warning => "#f59e0b",
        Level::Info => "#2563eb",
        Level::Debug => "#64748b",
        Level::Trace => "#94a3b8",
        Level::Unknown => "#1f2937",
    }
}
```

Update `render_heatmap_svg` to call `level_color(...)` instead of its internal
closure (verify `tests/test_heatmap.rs` still passes — the SVG strings must not
change).

### 2. `src/core/pipeline.rs` — heatmap data without file write

Add (import `crate::core::heatmap::{build_heatmap, Heatmap}`):

```rust
/// Ingest `inputs`, apply time + source filters (levels/search ignored — the
/// heatmap shows severity distribution), and return the built `Heatmap`.
pub fn run_heatmap_data(
    inputs: &[PathBuf],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    sources: Option<Vec<String>>,
    num_buckets: usize,
) -> Result<Heatmap, IngestError>
```

Refactor `run_heatmap` to call `run_heatmap_data` then
`render_heatmap_svg` + `std::fs::write` (its return value and behavior must be
unchanged — the CLI depends on it).

### 3. `src/gui/app.rs` — inline heatmap

**State:** add to `LogScopeApp`:
- `buckets: String` (default `"60"`).
- `heatmap: Option<crate::core::heatmap::Heatmap>` (default `None`).

**`start_export()`:**
- Parse `buckets`: trim; empty → 60; else `parse::<usize>()`; on error set
  `self.error = Some(format!("Invalid buckets: {t}"))` and return.
- The background thread currently computes `PipelineResult`. Extend it to ALSO
  compute the heatmap. Change `WorkerMsg::Done` payload to carry both. Simplest:
  define a small struct and change the enum:

```rust
struct WorkerOutput {
    result: PipelineResult,
    heatmap: Option<crate::core::heatmap::Heatmap>,
}
enum WorkerMsg {
    Done(Result<WorkerOutput, String>),
}
```

Inside the thread, after a successful `run_pipeline_with_options`, also call
`run_heatmap_data(&[input], start, end, sources.clone(), buckets)`; wrap the
whole thing so that an export success with a heatmap failure still returns
`Ok(WorkerOutput { result, heatmap: None })` (heatmap is non-fatal). An export
failure stays `Err(msg)`. `sources` is the `Option<Vec<String>>` already parsed
in `start_export()` — clone it before moving into the thread.

**`poll_worker()`:** on `Ok(out)`, set `self.heatmap = out.heatmap;` alongside the
existing status/preview update. On `Err`, set `self.heatmap = None`.

**Rendering:** in the `CentralPanel` block, ABOVE the text preview, if
`let Some(hm) = &self.heatmap` is set and `!hm.sources.is_empty()`, render the
grid; otherwise (empty heatmap) show a dim `"no events"` label. Implement a
method `fn render_heatmap(&self, ui: &mut Ui, hm: &Heatmap)`:

Layout (fixed, simple):
- `CELL_W = 6.0`, `ROW_H = 14.0`, `LABEL_W = 140.0`, `BOTTOM = 18.0`.
- `grid_w = hm.num_buckets as f32 * CELL_W`; `grid_h = hm.sources.len() as f32
  * ROW_H`. Allocate a canvas of size `(LABEL_W + grid_w + 8.0, grid_h +
  BOTTOM + 4.0)` via `ui.allocate_painter(egui::vec2(w, h), egui::Sense::hover())`.
- Get the painter for that rect. Paint cells as filled rects: for each source
  row `i` and bucket `j` where `hm.cells[i][j] != Level::Unknown`, draw
  `painter.rect_filled(rect, 0.0, color)` with `rect = Rect::from_min_size(
  pos2(x, y), vec2(CELL_W - 1.0, ROW_H - 1.0))`. Convert hex → `egui::Color32`
  with a small local helper `fn hex_color(&self, s: &str) -> Color32` (strip
  leading `#`, parse 2-hex-digit R/G/B via `u8::from_str_radix`, fallback
  `Color32::GRAY` on parse failure).
- Source labels: `painter.text(pos2(origin_x + LABEL_W - 6.0, row_center_y),
  egui::Align2::RIGHT_CENTER, truncated_source, egui::FontId::proportional(11.0),
  Color32::from_gray(220))`. Truncate source to 22 chars + `…` if longer.
- Time axis: three labels (min/mid/max) under the grid using
  `hm.min_ts`/`hm.max_ts` formatted `%Y-%m-%d %H:%M` (UTC), font 9, gray,
  anchored start/center/end at the grid's left/center/right x.
- Legend: below the labels, one swatch+text per level in order Critical, Error,
  Warning, Info, Debug, Trace (a horizontal row of small filled rects + short
  labels), font 10. If there is not enough vertical room, it is acceptable to
  omit the legend (the axis is the priority) — but include it since the canvas
  height already reserves `BOTTOM`; extend the reserve to `BOTTOM + 16.0` when
  drawing the legend, or place the legend in its own row via a separate
  `ui.horizontal(...)` below the canvas (PREFERRED — use a `ui.horizontal` row
  of `colored_label`-style swatches instead of painter text; egui's
  `ui.colored_label(color, "Error")` renders a color swatch + text for free).

egui API notes (0.36): use `ui.allocate_painter(size, sense)` which returns
`(Response, Painter)` — check the exact 0.36 signature; if it returns only the
painter or a `(Rect, Response)`, adapt. `egui::vec2`, `egui::pos2`,
`egui::Rect::from_min_size`, `egui::Align2`, `egui::FontId::proportional`,
`painter.rect_filled`, `painter.text`, and `Color32::from_rgb` are all stable in
0.36. If `allocate_painter` is not found, use
`let (rect, _) = ui.allocate_exact_size(egui::vec2(w,h), egui::Sense::hover());
let painter = ui.painter_at(rect);` (or `ui.painter()` scoped to a
`ui.allocate_ui` child). Do NOT panic at runtime; the UI must survive.

**Controls:** in `controls()`, add a `Buckets:` labeled single-line text edit
for `self.buckets` in the existing filters row (or a new row). Keep all existing
fields working.

## Tests

- `tests/test_heatmap.rs`: add a case asserting `level_color(Level::Error) ==
  "#dc2626"`, `level_color(Level::Unknown) == "#1f2937"`, and
  `level_color(Level::Critical) == "#7f1d1d"`.
- `tests/test_pipeline.rs` (or a new `tests/test_heatmap.rs` case): build a
  temp file with a couple of entries, call `run_heatmap_data(&[path], start,
  end, None, 10)` and assert `Ok(heatmap)` with `heatmap.sources.len() >= 1`,
  `heatmap.num_buckets == 10`, and `min_ts.is_some()`. Also assert
  `run_heatmap_data` on a nonexistent input returns `Ok` with an empty heatmap
  (`sources.is_empty()`, `min_ts.is_none()`) — matching the "nonexistent → empty,
  not Err" ingest contract.
- Existing `tests/cli.rs` heatmap test must stay green (run_heatmap still writes
  SVG). Do NOT change the CLI.

## Verification (coder MUST run)

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test    # ALL green (existing 73 + new)
cargo build   # BOTH binaries compile (logscope-gui must build)
```

Do NOT touch: `export.rs`, `normalize.rs`, `parser.rs`, `models.rs`, `level.rs`,
`evtx.rs`, `ingest.rs`, `filter.rs`, `datetime.rs`, `theme.rs`, `lib.rs`,
`logscope_cli.rs`, `logscope_gui.rs`, or `.github/`. No Cargo.toml changes.

## Report format (exact)

1. Files created/changed.
2. `cargo test` total passed + new-test count.
3. `cargo build` result (confirm BOTH binaries).
4. Confirm `cargo --version` printed 1.98.1.
5. Any deviation, especially the exact `allocate_painter`/`painter_at` API you
   used and whether you had to adapt from the plan.
