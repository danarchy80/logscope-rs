# logscope-rs Workspace Feature

## Goal
Replace the GUI's single `Input:` path with a **workspace**: an ordered list of
log inputs (files, `.zip`/`.tar*` archives, or folders) that the user can add
and remove. All filters, the heatmap, and Export operate on the merged sources
of the whole workspace.

## Why a per-item source list (not just path strings)
To remove an input unambiguously, each workspace item must retain the parsed
`Vec<LogSource>` it produced. Two different folders can both contain a file
named `app.log`; retaining per-item sources lets removal drop exactly the
entries from that input, independent of source-name collisions.

## Files
1. NEW `src/core/workspace.rs` — testable `Workspace` / `WorkspaceItem`.
2. `src/core/mod.rs` — register `pub mod workspace;`.
3. `src/gui/app.rs` — replace single-input fields with workspace state + UI.
4. NEW `tests/test_workspace.rs` — integration tests.

---

## 1. `src/core/workspace.rs`

```rust
//! A workspace is an ordered collection of log inputs and their parsed sources.
//! Items are retained separately so removal drops exactly the sources that
//! input produced (unaffected by source-name collisions across inputs).

use crate::models::LogSource;

#[derive(Debug, Clone)]
pub struct WorkspaceItem {
    /// Display path of the input as the user added it (file, archive, or folder).
    pub path: String,
    /// Parsed sources produced by ingesting `path`.
    pub sources: Vec<LogSource>,
}

#[derive(Debug, Clone, Default)]
pub struct Workspace {
    items: Vec<WorkspaceItem>,
}

impl Workspace {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// True if `path` is already present (exact trimmed-string match).
    pub fn contains(&self, path: &str) -> bool {
        self.items.iter().any(|i| i.path == path)
    }

    /// Append an item. No dedup here — callers decide.
    pub fn add(&mut self, path: String, sources: Vec<LogSource>) {
        self.items.push(WorkspaceItem { path, sources });
    }

    /// Remove by index; returns the removed item or None if out of range.
    pub fn remove(&mut self, index: usize) -> Option<WorkspaceItem> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[WorkspaceItem] {
        &self.items
    }

    /// Flattened view of every item's sources, in item order.
    pub fn merged_sources(&self) -> Vec<LogSource> {
        self.items
            .iter()
            .flat_map(|i| i.sources.clone())
            .collect()
    }
}
```

`LogSource` is already `Clone` (verify in `src/models.rs`; it is — `#[derive(Debug, Clone, PartialEq, Eq)]`).

## 2. `src/core/mod.rs`
Add `pub mod workspace;` (keep alphabetical order after `pipeline`).

---

## 3. `src/gui/app.rs` changes

### 3a. Imports
Add: `use crate::core::workspace::{Workspace, WorkspaceItem};`

### 3b. `WorkerMsg`
Replace
```rust
enum WorkerMsg { Done(Result<Vec<LogSource>, String>) }
```
with
```rust
enum WorkerMsg {
    /// One entry per ingested path: (display path, result).
    Done(Vec<(String, Result<Vec<LogSource>, String>)>),
}
```

### 3c. Struct fields
REMOVE: `input_path: String`, `loaded_path: String`, `all_sources: Vec<LogSource>`.

ADD:
```rust
/// Ordered inputs and their parsed sources. Single source of truth.
workspace: Workspace,
/// Cached flattened view of `workspace.merged_sources()`; rebuilt on add/remove.
all_sources: Vec<LogSource>,
```

`Default::default()`: `workspace: Workspace::new(),` and `all_sources: Vec::new(),`. Remove the now-unused `input_path` / `loaded_path` initializers.

### 3d. Remove the old single-path `load()` method entirely.
Replace it and `browse_file`/`browse_folder` with:

```rust
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
```

### 3e. `poll_worker`
Replace the `Ok(sources) => { ... loaded_path = ...; apply_filters(true); }` branch with:

```rust
Ok(results) => {
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
}
```
Keep `self.running = false; self.rx = None;` and `ctx.request_repaint();` as already present.

### 3f. `controls()`
Replace the FIRST `ui.horizontal` block (the one showing `Input:` label, input text edit, `Browse…`, `Folder…` buttons) with:

```rust
ui.horizontal(|ui| {
    let add_files = ui.button("Add Files…");
    let add_folder = ui.button("Add Folder…");
    let clear = ui.button("Clear");
    let enabled = !self.running;
    ui.add_enabled(enabled, egui::Button::new("")); // no-op placeholder — DO NOT USE; see note below
});
```
**NOTE (coder):** the line above is wrong — ignore the `add_enabled` placeholder line. Implement instead:

```rust
ui.horizontal(|ui| {
    if ui.add_enabled(!self.running, egui::Button::new("Add Files…")).clicked() {
        self.add_files();
    }
    if ui.add_enabled(!self.running, egui::Button::new("Add Folder…")).clicked() {
        self.add_folder();
    }
    if ui.add_enabled(!self.running, egui::Button::new("Clear")).clicked() {
        self.clear_workspace();
    }
});
ui.label(egui::RichText::new("Files, .zip/.tar archives, and folders are all supported.")
    .color(Color32::from_gray(140)));
```

Then, immediately after that block and before the Start/End row, render the workspace list:

```rust
if self.workspace.is_empty() {
    ui.label(egui::RichText::new("Workspace is empty — add files or a folder.")
        .color(Color32::from_gray(140)));
} else {
    for idx in 0..self.workspace.len() {
        let item = &self.workspace.items()[idx];
        let total: usize = item.sources.iter().map(|s| s.entries.len()).sum();
        ui.horizontal(|ui| {
            let label = format!("{}  ({} sources, {} entries)", item.path, item.sources.len(), total);
            ui.label(label);
            if ui.small_button("✖ Remove").clicked() {
                self.remove_item(idx);
            }
        });
    }
}
```
The `Remove` buttons must be disabled while running: wrap with `ui.add_enabled_ui(!self.running, |ui| { ... })` for the remove button (keep the label always shown).

Keep all remaining controls (Start/End, Output, Sources/Search/Buckets, Levels, Export) unchanged EXCEPT:

### 3g. Export button
Replace the `if self.loaded_path.trim() != self.input_path.trim() { self.load() } else { apply_filters(true) }` logic with a simple `self.apply_filters(true);`. The workspace already holds merged sources; there is no re-load step. If the workspace is empty, `apply_filters(true)` will export an empty file — acceptable, but also set a hint: before the button, no change needed.

### 3h. `logic()`
Keep the `if !self.all_sources.is_empty()` guard and fingerprint key as-is (it already only fingerprints start/end/sources/search/levels/buckets). Adding/removing items already triggers `apply_filters(true)` directly in `poll_worker`/`remove_item`, so no fingerprint change is required.

---

## 4. `tests/test_workspace.rs`

Mirror the helper style of `tests/test_export.rs`:

```rust
use logscope::core::workspace::Workspace;
use logscope::models::{Level, LogEntry, LogSource};

fn src(name: &str, n: usize) -> LogSource {
    let entries = (0..n).map(|i| LogEntry {
        timestamp: chrono::Utc::now(),
        raw_lines: vec![format!("line {i}")],
        source: name.to_string(),
        original_line_number: i,
        level: Level::Info,
    }).collect();
    LogSource { name: name.to_string(), entries }
}
```

Tests:
1. `empty_workspace` — `Workspace::new()` is empty; `len()==0`, `is_empty()`, `merged_sources()` empty.
2. `add_and_merge` — add two items `a` (1 source, 2 entries) and `b` (2 sources, 1 entry each); `len()==2`; `merged_sources()` has exactly 3 sources in item order.
3. `remove_drops_only_that_input` — two items each contributing a source named `app.log` (1 entry vs 2 entries); remove index 0; `merged_sources()` still has one `app.log` source with 2 entries.
4. `remove_out_of_range` — `remove(99)` returns `None` and `len()` unchanged.
5. `contains` — exact match true, different path false.
6. `clear` — after clear, empty and `merged_sources()` empty.

---

## Acceptance criteria
- `cargo test` compiles and ALL tests pass (76 existing + 6 new; do not break any existing test).
- `cargo build` succeeds for both binaries.
- No `input_path`/`loaded_path` references remain in `app.rs`.
- `git diff --stat` shows exactly the four files above changed.