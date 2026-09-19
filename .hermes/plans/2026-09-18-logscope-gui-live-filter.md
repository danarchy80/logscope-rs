# LogScope Rust: live-filtering GUI + clickable level toggles

Architect: high-cap model (Nous). Coder: agy (Gemini 3.1 Pro High). QA: high-cap model.

## Problem

Filtering logic is correct but the GUI only applies filters on the **Export**
button click, and the colored level legend is non-interactive. Users report:
1. "warning/info/error row does nothing" — it's a static legend that looks like
   clickable tabs.
2. "Search box does nothing" — search only takes effect after pressing Export;
   the preview and heatmap don't refresh live.
3. "Time fields should narrow records" — they DO (on Export) but not live.

Fix: make the GUI re-filter in-memory on every field edit (no Export needed),
and turn the level legend into real multi-select toggles.

## CRITICAL environment note

System `cargo` is apt 1.93.1 (too old). Use rustup 1.98.1 for EVERY cargo command:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version   # must print 1.98.1
```

## Context (read first)

- `src/gui/app.rs` (~495 lines): `LogScopeApp` holds input/start/end/output as
  strings, `levels_filter/sources_filter/search/buckets` strings, `running/rx/
  status/error/preview`, `heatmap: Option<Heatmap>`. `start_export()` parses the
  fields and spawns a background thread that runs `run_pipeline_with_options`
  (ingest→filter→export→counts) and, after, `run_heatmap_data`. `poll_worker`
  sets `status/preview/heatmap`. `render_heatmap()` paints the grid + a leading
  legend via `ui.colored_label(color, label)` — this is the non-interactive row.
  `load_preview()` reads the OUTPUT FILE back as text.
- `src/core/ingest.rs`: `pub fn ingest(path) -> Result<Vec<LogSource>, IngestError>`.
- `src/core/filter.rs`: `pub fn filter_entries_with_options(&[LogSource], &FilterOptions) -> Vec<LogEntry>`;
  `FilterOptions { start, end, levels: Option<Vec<Level>>, sources: Option<Vec<String>>, search: Option<String> }`.
- `src/core/export.rs`: `pub fn format_entries(&[LogEntry]) -> String`, `export_entries(&[LogEntry], path)`.
- `src/core/heatmap.rs`: `pub fn build_heatmap(&[LogEntry], num_buckets) -> Heatmap`, `render_heatmap_svg`.
- `src/models.rs`: `Level` enum + `Level::parse`/`as_str`. `LogEntry`/`LogSource` are `Clone`+`Send` (all owned).
- The custom `eframe::App` impl has `fn logic(&mut self, ctx)` and `fn ui(&mut self, ui, frame)` —
  `logic()` runs every frame. Keep that structure.

## Changes — all in `src/gui/app.rs` unless noted

### A. State changes

Replace `levels_filter: String` with:
```rust
use std::collections::HashSet;
selected_levels: HashSet<Level>,   // empty = all levels
```
Add:
```rust
all_sources: Vec<LogSource>,       // parsed entries held in memory after ingest
loaded_path: String,               // the input path that produced all_sources ("" = none)
filtered: Vec<LogEntry>,           // current live-filtered result
last_key: String,                  // filter fingerprint to detect edits
```
Remove `levels_filter` from `Default` (put `selected_levels: HashSet::new()`).
Keep `sources_filter/search/buckets` as strings. `preview` and `heatmap` stay.

Import `crate::core::ingest::ingest`, `crate::core::export::{format_entries, export_entries}`,
`crate::core::filter::{filter_entries_with_options, FilterOptions}`,
`crate::models::{Level, LogSource}`, and `std::collections::HashSet`.

### B. Worker thread → ingest only, then apply

Change `WorkerMsg` to carry the parsed sources:
```rust
enum WorkerMsg {
    Done(Result<Vec<LogSource>, String>),
}
```
`start_export()` becomes `load()`: validate input path non-empty; spawn a thread
that does `catch_unwind(|| ingest(&input).map_err(|e| e.to_string()))` and sends
`WorkerMsg::Done`. Do NOT run pipeline/heatmap in the thread anymore.

`poll_worker()`: on `Ok(sources)`, set `all_sources = sources`, `loaded_path = input_path.trim().to_string()`,
`running = false`, `rx = None`, THEN call `self.apply_filters(true)` (see C).
On `Err(msg)`, set `error = Some(msg)`, `running = false`.

### C. `apply_filters(write_file: bool)`

Synchronous. Parses the CURRENT field values and recomputes `filtered`,
`preview`, `heatmap`. Does NOT spawn threads.

1. Parse start/end with `parse_datetime` (empty string → far-past/far-future
   defaults, same as today). If either parse errors, set `self.error = Some(msg)`
   and return (leave previous filtered/preview intact). Validate `end >= start`.
2. Parse `sources_filter` (split ',', trim, drop empties) → `Option<Vec<String>>`.
3. Parse `search` (trim empty → None).
4. `let levels = if selected_levels.is_empty() { None } else { Some(selected_levels.iter().copied().collect::<Vec<_>>()) }`.
5. Build `FilterOptions { start, end, levels, sources, search }`; call
   `filter_entries_with_options(&self.all_sources, &opts)` → `self.filtered`.
6. `self.preview = Some(truncate_preview(format_entries(&self.filtered)))` — a new
   helper that truncates >10000 chars like the old `load_preview` (append "… (truncated)").
   Empty filtered → `preview = Some(String::new())`.
7. `self.heatmap = Some(build_heatmap(&self.filtered, buckets))` where buckets =
   parse of `self.buckets` (same rule as today; empty → 60; invalid → set error, return).
8. `self.status = format!("{} of {} entries from {} source(s)", filtered.len(),
   all_sources.iter().map(|s| s.entries.len()).sum::<usize>(), all_sources.len())`.
9. If `write_file`, `export_entries(&self.filtered, output)` (output path as today;
   empty → "unified.log"). On error set `self.error`.

### D. Live re-filter every frame

In `logic()`, after `poll_worker()`, add:
```rust
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
```
`selected_levels` is a `HashSet` — `{:?}` is deterministic enough here; a
HashSet's Debug order is stable for a fixed set of insertions but NOT guaranteed
ordered. To avoid a false "changed" every frame, instead build a sorted key:
sort the levels by `as_str()` before formatting, or store `selected_levels` as a
`BTreeSet<Level>` instead of `HashSet` (Level derives `Ord`). USE `BTreeSet` —
deterministic iteration and ordering. Import `std::collections::BTreeSet`.

### E. Load trigger

- When the Export button is clicked, do BOTH: if `loaded_path !=
  input_path.trim()` (input changed or nothing loaded), call `load()` to
  (re)ingest; if already loaded with the same path, just `apply_filters(true)`
  to write the file and refresh. Simplest correct rule:
  ```rust
  if self.loaded_path.trim() != self.input_path.trim() {
      self.load();          // ingest; poll_worker will apply_filters(true) when done
  } else {
      self.apply_filters(true);   // already loaded, just re-export current view
  }
  ```
- ALSO auto-load when the user changes the input field and it's non-empty and
  not yet loaded: in `logic()`, if `!self.input_path.trim().is_empty() &&
  self.loaded_path.trim() != self.input_path.trim() && !self.running`, call
  `self.load()`. (This makes the first file/folder pick load without an extra
  Export click — but Export still writes the file.) To avoid re-ingesting on
  every keystroke of a path being typed, trigger auto-load only when the path
  string ends a "stable" editing session — egui correct trigger is
  `response.lost_focus() && response.changed()` on the input TextEdit. Implement:
  in `controls()`, capture the input field `Response`; if
  `resp.lost_focus() && resp.changed() && !self.input_path.trim().is_empty()`,
  call `self.load()`. Same for the Browse/Folder buttons (they set input_path,
  then call `load()` directly).

### F. Interactive level toggles (fixes "does nothing" complaint)

Replace the passive legend at the end of `render_heatmap()` with clickable
toggles that edit `self.selected_levels`. Because `render_heatmap(&self, ...)`
takes `&self`, change the method signature to `&mut self`. For each level in
```rust
[Level::Critical, Level::Error, Level::Warning, Level::Info, Level::Debug, Level::Trace]
```
render a toggle chip: `ui.selectable_label(selected, format!("◼ {label}"))` where
`selected = self.selected_levels.contains(&lvl)` and label = human name. On
`.clicked()`, toggle membership in `self.selected_levels`. Color the swatch/label
with `self.hex_color(level_color(lvl))` (use `RichText::color`). Add a small hint
label before the row: "Filter by level (click):". These toggles ARE the level
filter now; the old "Levels (comma):" text field is REMOVED from `controls()`.

When `selected_levels` is empty, all levels pass (matches current CLI `None` =
all). Toggling one on filters to that level; toggling more ORs them; toggling all
off returns to "all".

### G. Remove now-dead code

- Delete `load_preview()` (replaced by in-memory `format_entries` truncation).
- Delete the `levels_filter` parsing block in `start_export` (replaced by BTreeSet).
- The heatmap empty-state label ("no events") stays; the level legend inside
  `render_heatmap` is replaced by the toggles.

Keep: `hex_color`, `render_heatmap` grid painting + time axis, `browse_*`,
`controls` for input/start/end/output/sources/search/buckets, the Export button,
spinner, error/status rendering, preview panel.

## Tests (add; do not break existing 75)

Core logic is unchanged, so pure-GUI changes have no unit tests. But add ONE
regression test proving filter options work via the public API the GUI now
depends on (levels + search combined), in `tests/test_filter.rs`:
```rust
#[test]
fn filter_entries_with_options_levels_and_search_combined() {
    // parse a small file with an ERROR line "db timeout" and an INFO line "ok";
    // FilterOptions { levels: Some(vec![Error]), search: Some("timeout") }
    // matches exactly the ERROR line; assert len == 1.
    // Also assert empty levels (Some(vec![])) behaves like None (returns all).
}
```
Existing behavior: `levels: Some(vec![])` currently returns NOTHING (contains
check on empty vec). Confirm what the code does and do NOT change core filter
semantics — the GUI sends `None` when the set is empty, so this is safe. In the
test, assert the CURRENT behavior so it documents the contract; do not "fix" it.

## Verification (coder MUST run)

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test      # all green (existing 75 + 1 new)
cargo build     # BOTH binaries (gui must compile with all the &mut self / BTreeSet changes)
```

Do NOT touch core modules (`filter.rs`, `ingest.rs`, `export.rs`, `heatmap.rs`,
`pipeline.rs`, `parser.rs`, `models.rs`, `level.rs`, `evtx.rs`), the CLI, or
`.github/`. This is a `src/gui/app.rs` change plus one test file. No Cargo.toml
change.

## Report format (exact)

1. Files changed.
2. `cargo test` total passed + new-test count.
3. `cargo build` result (both binaries).
4. Confirm `cargo --version` = 1.98.1.
5. Describe the final interaction model in 2-3 sentences (when does ingest
   happen, when does the view refresh, what Export does now), and any deviation.