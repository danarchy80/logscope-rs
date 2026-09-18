# LogScope Rust: search filter + SVG severity heatmap

Architect: high-cap model (Nous). Coder: agy (Gemini 3.1 Pro High). QA: high-cap model.

## Objective

Add two features to `/home/danarchy/workspace/logscope-rs`:

1. **Search filter** — case-insensitive substring match over each entry's full
   text (all raw lines joined). New `--search <text>` CLI flag, threaded through
   `FilterOptions` / `PipelineOptions`, and a GUI text field.
2. **SVG severity heatmap** — a new `--heatmap <out.svg>` CLI flag that writes a
   standalone SVG grid: rows = sources (systems), columns = time buckets, each
   cell colored by the MAX severity level in that (source, bucket). Zero new
   dependencies (SVG is hand-built string output).

All existing behavior (56+ tests, level/source/time filters, evtx, multi-input,
unified export) must stay green.

## CRITICAL environment note

System `cargo` on PATH is apt 1.93.1 (too old). Use rustup 1.98.1 for EVERY
cargo command:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version   # must print 1.98.1
```

## Part 1 — Search filter

### `src/core/filter.rs`
Add a `search: Option<String>` field to `FilterOptions` (documented:
"case-insensitive substring over the entry's joined raw lines; None = no
search"). In `filter_entries_with_options`, add a predicate: if `search` is
`Some(needle)`, keep the entry only when
`entry.raw_lines.join("\n").to_lowercase().contains(&needle.to_lowercase())`.

`filter_entries` (the old 3-arg wrapper) must still compile — it constructs a
`FilterOptions`, so add `search: None` there.

### `src/core/pipeline.rs`
Add `search: Option<String>` to `PipelineOptions` and forward it into the
`FilterOptions` in `run_pipeline_with_options`. Update `run_pipeline` (the old
wrapper) to pass `search: None`.

### `src/bin/logscope_cli.rs`
Add `#[arg(long = "search")] search: Option<String>`. Pass it into
`PipelineOptions`. Update the `#[command(about = ...)]` help text to mention
`--search`.

### `src/gui/app.rs`
Add a `search: String` state field (default empty) and a "Search:" text box in
`controls()`. In `start_export()`, convert empty/whitespace-only to `None`, else
`Some(trimmed)`, and pass through `PipelineOptions`. Empty string = no filter
(same semantics as CLI).

## Part 2 — SVG heatmap

### `src/core/heatmap.rs` (NEW) — register `pub mod heatmap;` in `src/core/mod.rs`

Exact types (use these signatures verbatim):

```rust
use chrono::{DateTime, Utc};
use crate::models::{Level, LogEntry};

/// Severity rank for max-severity cell coloring. Unknown = 0 (empty cell).
fn severity_rank(l: Level) -> u8 {
    match l {
        Level::Critical => 6,
        Level::Error => 5,
        Level::Warning => 4,
        Level::Info => 3,
        Level::Debug => 2,
        Level::Trace => 1,
        Level::Unknown => 0,
    }
}

pub struct Heatmap {
    pub sources: Vec<String>,          // sorted alphabetically, deduped
    pub num_buckets: usize,
    pub min_ts: Option<DateTime<Utc>>, // None iff entries empty
    pub max_ts: Option<DateTime<Utc>>,
    pub cells: Vec<Vec<Level>>,        // [source_index][bucket_index]
}

/// Build a heatmap from entries. Timeline = [min_ts, max_ts] of the entries
/// (data-driven). Sources sorted alphabetically (stable, deterministic SVG).
/// Each cell = max severity (by severity_rank) of entries in that bucket.
/// Empty entries -> empty sources, num_buckets preserved, min/max None.
pub fn build_heatmap(entries: &[LogEntry], num_buckets: usize) -> Heatmap

/// Render the heatmap as a standalone SVG string.
pub fn render_heatmap_svg(h: &Heatmap) -> String
```

`build_heatmap` bucket assignment: if `max_ts == min_ts`, every entry goes to
bucket 0; otherwise
`bucket = ((ts - min_ts).num_milliseconds() * num_buckets as i64 / (max_ts - min_ts).num_milliseconds()) as usize`,
clamped to `num_buckets - 1`. Use `chrono::Duration::num_milliseconds` /
`.checked_sub` (avoid overflow) — timestamps are all `DateTime<Utc>` already.

`render_heatmap_svg` layout (dark theme, standalone — no external CSS/fonts):

- Cell width `CELL_W = 8`, row height `ROW_H = 18`, left label margin `MARGIN_LEFT = 160`,
  top margin `MARGIN_TOP = 30`, right/bottom margin `24`.
- Total width = `MARGIN_LEFT + num_buckets * CELL_W + 24`.
- Total height = `MARGIN_TOP + sources.len() * ROW_H + 40` (bottom axis + legend).
- Fill color by level (EXACT — use these literals):
  - `Critical`: `#7f1d1d`
  - `Error`: `#dc2626`
  - `Warning`: `#f59e0b`
  - `Info`: `#2563eb`
  - `Debug`: `#64748b`
  - `Trace`: `#94a3b8`
  - `Unknown` (empty cell): `#1f2937` (grid background)
- Each non-empty cell is a `<rect x=.. y=.. width=CELL_W-1 height=ROW_H-1 fill=color/>`.
  Empty cells are NOT drawn (the `<svg>` background is the grid color).
- Y-axis: one `<text>` per source, `x=MARGIN_LEFT-8`, `text-anchor=end`,
  vertical-centered in its row, `fill=#e5e7eb`, font-size 12, truncated to 30
  chars with `…` if longer.
- X-axis: three time labels (start, middle, end) as `YYYY-MM-DD HH:MM` (UTC),
  font-size 10, `fill=#9ca3af`, at x = left/center/right of the grid.
- Title text at top-left: `"LogScope heatmap — N sources × M buckets"` fill `#f9fafb`.
- Legend: a horizontal strip below the grid: one color swatch + label per level
  in order Critical, Error, Warning, Info, Debug, Trace (skip Unknown). Swatch =
  `<rect>` 12×12 with the level color; label text beside it, fill `#e5e7eb`.
- Root element: `<svg xmlns="http://www.w3.org/2000/svg" width=".." height=".."
  font-family="monospace">` and a `<rect>` background of `#0b1220` covering the
  whole canvas, drawn FIRST.
- Empty heatmap (`min_ts.is_none()`): return a minimal valid SVG with the
  background rect and a single centered `<text>` `"no events"` fill `#9ca3af`.

Do NOT use any SVG library or new dependency. Build the string with `format!`
and `push_str`.

### `src/core/pipeline.rs` — heatmap runner

Add (imports from `crate::core::heatmap`):

```rust
/// Ingest `inputs`, apply time + source filters (levels/search ignored — the
/// heatmap shows severity distribution), build an SVG heatmap with
/// `num_buckets` columns, and write it to `output_path`. Returns the number of
/// entries represented.
pub fn run_heatmap(
    inputs: &[PathBuf],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    sources: Option<Vec<String>>,
    num_buckets: usize,
    output_path: &Path,
) -> Result<usize, IngestError>
```

Implementation: ingest each input (reuse `ingest`), concat sources, then
`filter_entries_with_options` with `FilterOptions { start, end, levels: None,
sources, search: None }`, then `build_heatmap(&filtered, num_buckets)`,
`render_heatmap_svg`, `std::fs::write(output_path, svg)?`, return
`filtered.len()`. Map io error into `IngestError` (it has an `Io` variant via
`#[from] std::io::Error`).

### `src/bin/logscope_cli.rs` — heatmap flag

Add:
- `#[arg(long = "heatmap")] heatmap: Option<PathBuf>` — SVG output path.
- `#[arg(long = "buckets", default_value_t = 60)] buckets: usize` — columns
  (used only with `--heatmap`).

In `main()`, after the existing `run_pipeline_with_options` call (keep it — the
unified log still exports), IF `cli.heatmap` is `Some(path)`, call
`run_heatmap(&cli.input, start, end, sources.clone(), cli.buckets, &path)` and
on success `println!("Heatmap written to {path}")`; on error `eprintln!("error:
{e}"); exit(1);`. The `sources` variable already exists (Option<Vec<String>>) —
clone it. Note: `start`/`end` are already validated (end >= start) earlier.

## Tests (REQUIRED)

### New `tests/test_heatmap.rs`
Import `logscope::core::heatmap::{build_heatmap, render_heatmap_svg}` and
`logscope::models::{Level, LogEntry}`. Build entries with `chrono::Utc`.
Cases:
1. `build_heatmap` over 3 entries (2 sources × 2 buckets) → assert `sources`
   sorted, `num_buckets` correct, and the max-severity cell values: e.g. an
   Error+Info in the same bucket → cell == `Level::Error`.
2. `build_heatmap` with all equal timestamps → all entries land bucket 0, no panic.
3. `build_heatmap` empty → `sources` empty, `min_ts`/`max_ts` None, `cells` empty.
4. `render_heatmap_svg` non-empty → contains `"<svg"`, contains each source
   name, contains `"#dc2626"` (error color) when an Error cell exists, contains
   a legend label `"Critical"`.
5. `render_heatmap_svg` empty → contains `"no events"`.
6. Determinism: two `render_heatmap_svg` calls on the same `Heatmap` are equal.

### Extend `tests/test_filter.rs`
Add `filter_entries_with_options` cases:
- `search` Some("timeout") → matches only entries whose text contains "timeout"
  (case-insensitive: "TIMEOUT" also matches).
- `search` Some("nomatchzzz") → empty.

### Extend `tests/cli.rs`
- `--search` narrows the 6-entry SIMPLE_LOG to only entries containing a word.
- `--heatmap out.svg` writes a file whose content starts with `"<svg"`.

## Verification (coder MUST run before reporting)

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test    # ALL green (existing 63 + new)
cargo build   # both binaries compile
```

Do NOT touch `export.rs`, `normalize.rs`, `parser.rs`, `models.rs`, `level.rs`,
`evtx.rs`, `datetime.rs`, `theme.rs`, `lib.rs`, `logscope_gui.rs`, or `.github/`.
Do NOT add any dependency to Cargo.toml.

## Report format (exact)

1. Files created/changed.
2. `cargo test` — total passed + count of new tests.
3. `cargo build` result.
4. Confirm `cargo --version` printed 1.98.1.
5. Any deviation or remaining failure.
