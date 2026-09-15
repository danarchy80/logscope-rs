# LogScope Rust: GUI (egui/eframe) + App Icon

Architect: high-cap model (Nous). Coder: EVO-X3 (halogen-qwen3.8-flash-next).
QA: high-cap model.

## Goal

Replace the CLI-only `LogScope.exe` with a real windowed desktop app that
stays open on double-click, and give it a proper icon. Keep the CLI working
unchanged as a secondary binary.

## Context / current state

Crate at `/home/danarchy/workspace/logscope-rs`. Current binaries:
- `logscope-cli` (src/bin/logscope_cli.rs) — clap CLI, `parse_datetime` is a
  `pub fn` in THIS binary file. Core lib = `logscope::{core, models}` with
  `run_pipeline`, `PipelineResult`, `IngestError`, `ingest`.

Icon files already generated:
- `assets/logscope_icon.ico` (9 sizes, valid ICO)
- `assets/logscope_icon.png` (1024x1024 master)

## Changes

### 1. Shared datetime parser → move into the library

The CLI's `parse_datetime` (RFC3339 / `%Y-%m-%d %H:%M:%S` /
`%Y-%m-%dT%H:%M:%S`, returns `DateTime<Utc>`, human-readable `Err(String)`)
should live in the LIBRARY so both CLI and GUI reuse it. Create
`src/datetime.rs` with `pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>,
String>` (copy the exact implementation from src/bin/logscope_cli.rs). Register
`pub mod datetime;` in `src/lib.rs`. Then DELETE the private copy in
`src/bin/logscope_cli.rs` and import the lib version instead (`use
logscope::datetime::parse_datetime;`). Do NOT change its behavior or error
message text. Do NOT break the CLI tests in `tests/cli.rs`.

### 2. GUI app (egui/eframe)

New binary `logscope-gui` at `src/bin/logscope_gui.rs`, new module
`src/gui/mod.rs` (or `src/gui.rs` + `src/gui/app.rs` — pick one clean layout).

Dependencies to add via `cargo add`:
- `eframe` (latest 0.3x — this pulls egui; use the version cargo resolves)
- `egui` (same version eframe uses)
- `rfd` (native file dialogs — file, folder, and save dialogs)

Add a `[[bin]]` entry in Cargo.toml:
```toml
[[bin]]
name = "logscope-gui"
path = "src/bin/logscope_gui.rs"
```

GUI behavior (reproduces the Python Qt app's function, not its exact layout):

- Top of `main()`: `#![windows_subsystem = "windows"]` — CRITICAL so the exe is
  a windowed (not console) binary on Windows; this is what stops the
  "flash then nothing" console popup.
- Window title: "LogScope — Unified Log Exporter". Default size ~720x560.
- Fields, each with a label:
  1. **Input path** (text box) + "Browse…" button → `rfd::FileDialog` (single
     file) and a small "Folder…" button → `rfd::FileDialog::new().pick_folder()`.
     The core `ingest` auto-detects file/folder/zip/tar from the path, so one
     combined input path field is enough — but keep both File and Folder buttons
     for convenience. (Zip/tar are handled automatically by suffix.)
  2. **Start** and **End** datetime text boxes (`YYYY-MM-DD HH:MM:SS`),
     default Start = now minus 1 day at 00:00:00, End = now. Empty/blank start
     or end → use the same far-past (1970) / far-future (9999) defaults as the
     CLI. Parse with the shared `parse_datetime`; show inline error text if
     invalid.
  3. **Output path** text box (default `unified.log`) + "Save…" button →
     `rfd::FileDialog::new().save_file()`.
- **Export** button → runs `run_pipeline` on a BACKGROUND thread (do NOT block
  the UI): spawn a `std::thread`, send the `Result<PipelineResult,
  IngestError>` back over `std::sync::mpsc::channel`. While running, show a
  spinner (`egui::Spinner`) and disable the Export button. `ctx.request_repaint()`
  in a non-blocking poll or use the channel's `try_recv` each frame in
  `update()`.
- On success: status label `Done — {filtered} of {total} entries from {sources}
  source(s) exported.` and load the output file into a read-only, scrollable
  preview panel (`egui::ScrollArea` over `ui.monospace(...)`, text truncated
  to ~10000 chars with an ellipsis note like the Python version).
- On error: show the error string in the status label (and ideally a small red
  error line). Do NOT panic — the UI must survive and stay open.
- A thin top area or `menu` is fine; keep it simple. Do NOT over-engineer a
  fancy layout. The acceptance bar is: window opens, stays open, Export runs a
  real pipeline in the background with a progress state, results show in a
  preview.

### 3. Embed the icon

Two parts (both use the existing assets/ files):

(a) **Executable file icon** (what Windows Explorer/taskbar-snapshot uses):
    add a `build.rs` at the crate root:
    ```rust
    fn main() {
        #[cfg(target_os = "windows")]
        embed_resource::compile("assets/icon.rc", embed_resource::NONE);
    }
    ```
    and add `[build-dependencies]` `embed-resource = "3"` (cargo add --build
    embed-resource). Create `assets/icon.rc`:
    ```
    id ICON "assets/logscope_icon.ico"
    ```
    (embed_resource resolves paths relative to the crate root; if a leading
    path fails, use the bare filename `logscope_icon.ico` and confirm the cwd.)
    This makes the compiled .exe (both CLI and GUI) carry the icon.

(b) **Runtime window icon** (taskbar/alt-tab while running): in the GUI,
    decode `assets/logscope_icon.png` to RGBA at runtime (use the `image` crate:
    `cargo add image`, then `image::load_from_memory(include_bytes!("../assets/logscope_icon.png"))`
    → `to_rgba8()` → build `egui::IconData` with the dimensions), and pass it to
    `eframe::NativeOptions`:
    ```rust
    viewport: egui::ViewportBuilder::default()
        .with_inner_size([720.0, 560.0])
        .with_icon(icon),
    ```
    If `with_icon` is not available on the resolved egui version, set the icon
    via `ctx`/`ViewportCommand::Icon` in the first `update()` frame and note it
    in your report.

## Do NOT touch / preserve

- `src/core/*` — parser, ingest, filter, normalize, export, pipeline unchanged.
- `src/models.rs` unchanged.
- CLI behavior + `tests/cli.rs` (56 tests currently passing) must stay green.
- `Cargo.lock` will change (new deps) — that's expected, commit it.

## Verification (coder must run)

- `cargo build` (Linux, no target) compiles BOTH binaries.
- `cargo test` — all existing tests still pass (56).
- `cargo clippy` is optional; don't block on warnings.

## Output format (report exactly)

1. Files created/changed.
2. `cargo test` result (expect 56 passed).
3. Exact `eframe`/`egui` versions cargo resolved, and whether `ViewportBuilder::with_icon`
   existed or you used the `ViewportCommand::Icon` fallback.
4. Which crate/file holds `parse_datetime` now, and confirm the CLI still
   imports it from the lib (no behavior change).
5. Any cargo warning or assumption (especially icon path in build.rs).