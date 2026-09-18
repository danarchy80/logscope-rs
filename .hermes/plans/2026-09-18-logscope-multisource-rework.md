# LogScope Rust: Multi-source rework — levels, source filter, Windows .evtx

Architect: high-cap model (Nous). Coder: agy (Gemini 3.1 Pro High) — broad multi-file
coordination, per role-based-coding skill. QA: high-cap model.

## Objective

Extend the Rust LogScope core (`/home/danarchy/workspace/logscope-rs`) with:

1. **Severity detection + level filtering** — detect Error/Warn/Critical/Warning/Info
   (plus Debug/Trace) on every entry, and filter by allowed levels.
2. **Source filtering** — include only entries whose source basename matches a
   case-insensitive substring list.
3. **Native Windows `.evtx` parsing** — via the `evtx` crate (0.12.x).
4. **Multiple input paths** — `run_pipeline` accepts several inputs (files/zips/folders)
   and merges them (CLI: repeatable `--input`).

Existing behavior (multi-line grouping, time-window filter, UTC normalization,
chronological merge, unified export) must be preserved byte-for-byte.

## CRITICAL environment note (read first)

The system `cargo`/`rustc` on PATH is apt's **1.93.1**, which is too old for the
egui deps. rustup 1.98.1 lives at `~/.cargo/bin`. **Every cargo command you run
must use the rustup toolchain:**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version   # must print 1.98.1
```

If you run bare `cargo` you will get `error: rustc 1.93.1 is not supported by ...
ecolor@0.36.2 requires rustc 1.95`. Do not downgrade deps — use the 1.98.1 PATH.

## Deliverable files (touch ONLY these + new files listed)

### A. `src/models.rs`
Add a `Level` enum and a `level` field on `LogEntry`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Critical,
    #[default]
    Unknown,
}

impl Level {
    /// "TRACE" | "DEBUG" | "INFO" | "WARNING" | "ERROR" | "CRITICAL" | "UNKNOWN"
    pub fn as_str(&self) -> &'static str { /* implement */ }

    /// Case-insensitive parse. Accepts "critical"|"fatal", "error"|"err",
    /// "warn"|"warning", "info", "debug", "trace". Returns None for anything else.
    pub fn parse(s: &str) -> Option<Level> { /* implement */ }
}
```

`LogEntry` gains `pub level: Level` (default `Level::Unknown`). `LogSource` unchanged.

### B. `src/core/level.rs` (NEW)
Register in `src/core/mod.rs` (`pub mod level;`).

```rust
use crate::models::Level;

/// Detect a severity level from an entry's full raw lines.
///
/// Priority order (first hit wins):
/// 1. Syslog `<PRI>`: if the FIRST line starts with `<digits>`, severity = PRI % 8.
///    Map 0..=2 -> Critical, 3 -> Error, 4 -> Warning, 5|6 -> Info, 7 -> Debug.
/// 2. Keyword regex, case-insensitive, checked in this order (most severe first):
///      Critical: \b(CRITICAL|FATAL|CRIT|ALERT|EMERG|EMERGENCY)\b
///      Error:    \b(ERROR|ERR)\b
///      Warning:  \b(WARN|WARNING)\b
///      Info:     \b(INFO|NOTICE)\b
///      Debug:    \b(DEBUG|TRACE|VERBOSE)\b
///    Scan across ALL lines joined with '\n'.
/// 3. Default Level::Unknown.
pub fn detect_level(raw_lines: &[String]) -> Level { /* implement */ }
```

Use the `regex` crate (already a dependency) with `Regex` statics via `LazyLock`.

### C. `src/core/parser.rs`
In `parse_file`, finalize the level when each entry is PUSHED (both push sites —
the mid-loop push when a new timestamp arrives, and the EOF push). Before
`source.entries.push(entry)` set `entry.level = detect_level(&entry.raw_lines);`.
This way continuation lines (stack traces) participate in detection.

### D. `src/core/evtx.rs` (NEW)
Register in `src/core/mod.rs`.

```rust
use evtx::EvtxParser;
use crate::core::ingest::IngestError;
use crate::models::{Level, LogEntry};

/// Map a Windows Event Log numeric level to our Level.
/// 1=Critical, 2=Error, 3=Warning, 4=Information->Info, 5=Verbose->Debug, else Unknown.
pub fn map_evtx_level(n: u32) -> Level { /* implement */ }

/// Parse EVTX bytes (XML rendering) into LogEntry list.
/// Top-level open/header failure -> Err(IngestError). Individual record parse
/// failures are SKIPPED (best-effort, mirrors archive-member handling).
/// Each record becomes ONE LogEntry:
///   timestamp = record.timestamp (jiff Timestamp) -> chrono DateTime<Utc>
///   level     = map_evtx_level(level parsed from "<Level>N</Level>" in the XML)
///   raw_lines = the record's XML text split on '\n' (Vec<String>)
///   source    = name, original_line_number = 0
pub fn parse_evtx(name: &str, bytes: &[u8]) -> Result<Vec<LogEntry>, IngestError> { /* implement */ }
```

Conversion details (from docs.rs evtx 0.12.2):
- `EvtxParser::from_buffer(bytes.to_vec())?` returns `EvtxParser<Cursor<Vec<u8>>>`.
- `parser.records()` yields `Result<SerializedEvtxRecord<Vec<u8>>, _>`; each Ok has
  `.event_record_id`, `.timestamp` (jiff `Timestamp`), `.data` (`Vec<u8>` = XML).
- jiff timestamp -> chrono: `record.timestamp.as_second()` (i64) and
  `record.timestamp.subsec_nanosecond()` (i32), then
  `chrono::DateTime::<Utc>::from_timestamp(secs, nanos).unwrap_or_default()`.
- XML is `String::from_utf8_lossy(&record.data)`.
- Level extraction: regex `<Level>\s*(\d+)\s*</Level>` on the XML; first capture is
  the numeric level; if absent use `Level::Unknown`.

Add `evtx = "0.12"` to `[dependencies]` in Cargo.toml (`cargo add evtx` — but do it
with the 1.98.1 PATH). Do NOT add jiff directly; it's transitive via evtx and the
`as_second`/`subsec_nanosecond` methods are on the re-exported type.

### E. `src/core/ingest.rs` — return-type refactor for evtx support

Introduce a public enum and change `extract_files` to return it:

```rust
#[derive(Debug)]
pub enum ExtractedFile {
    Text { name: String, lines: Vec<String> },
    Evtx { name: String, bytes: Vec<u8> },
}

fn is_evtx(name: &str) -> bool { name.to_lowercase().ends_with(".evtx") }

pub fn extract_files(path: &Path) -> Result<Vec<ExtractedFile>, IngestError> { /* implement */ }
```

`ingest` becomes:

```rust
pub fn ingest(path: &Path) -> Result<Vec<LogSource>, IngestError> {
    let files = extract_files(path)?;
    let mut sources = Vec::new();
    for f in files {
        match f {
            ExtractedFile::Text { name, lines } => sources.push(parse_file(&name, &lines)),
            ExtractedFile::Evtx { name, bytes } => {
                sources.push(LogSource { name: name.clone(), entries: parse_evtx(&name, &bytes)? });
            }
        }
    }
    Ok(sources)
}
```

Update every internal helper to emit `ExtractedFile`:
- `extract_single`: `.evtx` -> `Evtx{name, bytes: std::fs::read(path)?}`; else existing
  text branches (`Text{...}`).
- `walk_dir`: per file, `is_evtx(name)` -> read bytes as `Evtx`; else existing logic.
- `extract_zip`: member name ending `.evtx` -> `Evtx{name: basename, bytes: data}`;
  else `Text{name: basename, lines}`.
- `extract_tar`: same member-name branch.
- `read_lines` stays text-only (used only on the Text path now).

Legacy `.evt` (pre-Vista) is NOT supported (no maintained Rust crate) — `is_evtx`
returns false for `.evt`, so such files fall through to lossy text. Do not crash.

### F. `src/core/filter.rs` — source + level filters

Keep the existing `filter_entries(sources, start, end)` signature (tests depend on
it). Add:

```rust
#[derive(Debug, Clone)]
pub struct FilterOptions {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub levels: Option<Vec<Level>>,    // None = all levels
    pub sources: Option<Vec<String>>,  // None = all sources; else case-insensitive substring
}

pub fn filter_entries_with_options(sources: &[LogSource], opts: &FilterOptions) -> Vec<LogEntry>
```

Semantics: entry kept iff `opts.start <= ts <= opts.end` AND (levels is None OR
entry.level ∈ levels) AND (sources is None OR any pattern is a case-insensitive
substring of entry.source).

Refactor `filter_entries` to delegate to `filter_entries_with_options` with
`levels: None, sources: None`.

### G. `src/core/pipeline.rs` — multi-input + options

Keep existing `run_pipeline(input, start, end, output)` (tests depend on it); have it
delegate to the new function. Add:

```rust
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub levels: Option<Vec<Level>>,
    pub sources: Option<Vec<String>>,
}

pub fn run_pipeline_with_options(
    inputs: &[PathBuf],
    opts: &PipelineOptions,
    output_path: &Path,
) -> Result<PipelineResult, IngestError>
```

Multi-input: `ingest` each path, concat the `Vec<LogSource>`. `total_entries` =
sum over all sources. Filter via `filter_entries_with_options`. Export unchanged.
`PipelineResult` unchanged.

### H. `src/bin/logscope_cli.rs` — new flags

- Change `input` to `#[arg(short = 'i', long = "input", required = true)] input: Vec<PathBuf>`
  (clap accepts one-or-more `--input`). Update `run_pipeline` call to
  `run_pipeline_with_options(&input, &opts, &output)`.
- Add `#[arg(long = "level")] level: Vec<String>` — each value may be comma-separated
  ("error,warn"); also supports repeated `--level`. Parse each token with
  `Level::parse`; unknown token -> `eprintln!("error: --level: unknown level {t}"); exit(2)`.
  Empty list -> `None` (all levels).
- Add `#[arg(long = "source")] source: Vec<String>` — substring filters (no validation).
  Empty -> `None`.
- Build a `PipelineOptions { start, end, levels, sources }`.
- Keep the existing `--start`/`--end` parse + validation (`end < start` -> exit 2) and
  the success/error stdout/stderr messages EXACTLY as-is.

### I. `src/gui/app.rs` — minimal wire-up

Add two state fields (`levels_filter: String`, `sources_filter: String`, default empty)
and render two text boxes (labels "Levels (comma):" and "Sources (comma):") in
`controls()`. In `start_export()`, parse the levels string (split on ',', trim, empty
tokens skipped; unknown token -> set `self.error` and return) into
`Option<Vec<Level>>`, and sources string into `Option<Vec<String>>` (empty -> None).
Call `run_pipeline_with_options(&[input], &PipelineOptions { ... }, &output)`.
Multi-file in the GUI is served by the existing Folder… picker (folder = many logs);
do not add a multi-file list in this pass.

## Test migration (REQUIRED — the code will not compile otherwise)

Adding `level` to `LogEntry` breaks every struct-literal construction. Update:
- `tests/test_export.rs` — `entry()` helper (line 16) and the `LogEntry { .. }` at
  line 55: add `level: Level::Unknown` (import `logscope::models::Level`).
- `tests/test_normalize.rs` — two `LogEntry { .. }` literals (lines 54, 69): add
  `level: Level::Unknown`.
- `tests/ingest.rs` — imports `extract_files`; the return type changed from
  `Vec<(String, Vec<String>)>` to `Vec<ExtractedFile>`. The existing assertions at
  lines 136 and 189 call `.is_empty()` / `.is_err()` — still compile. But if any
  test pattern-matches the tuple shape, update to the enum. Grep and fix.

## New tests (add these)

1. `tests/test_level.rs` — `detect_level`:
   - syslog `<3>` -> Error, `<0>` -> Critical, `<4>` -> Warning, `<6>` -> Info, `<7>` -> Debug.
   - keyword: "ERROR" -> Error, "WARN" -> Warning, "CRITICAL" -> Critical,
     "warning" (lowercase) -> Warning, "INFO" -> Info, "DEBUG" -> Debug.
   - priority: "CRITICAL ERROR" -> Critical (not Error).
   - none -> Unknown.
   - `Level::parse`: "error" -> Error, "warn" -> Warning, "warning" -> Warning,
     "critical" -> Critical, "fatal" -> Critical, "info" -> Info, "bogus" -> None.
2. `tests/test_evtx.rs` — pure-function tests (no binary fixture):
   - `map_evtx_level(1)`=Critical, `(2)`=Error, `(3)`=Warning, `(4)`=Info, `(5)`=Debug,
     `(0)`/`(9)`=Unknown.
   - `parse_evtx` on garbage bytes returns Err (corrupt top-level, parity with corrupt gzip).
3. Extend `tests/test_filter.rs` — `filter_entries_with_options`:
   - level filter: keep only `Level::Error` entries.
   - source filter: keep only entries whose source contains "auth" (case-insensitive).

## Verification (coder MUST run before reporting)

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test          # ALL tests pass (existing 56 + new)
cargo build         # both binaries compile
```

Do NOT run `cargo clippy` or block on warnings. Do NOT touch `src/core/export.rs`,
`src/core/normalize.rs`, `src/datetime.rs`, `src/gui/theme.rs`, `src/lib.rs`,
`src/bin/logscope_gui.rs`, or the `.github/` workflows.

## Report format (exact)

1. Files created/changed (list each path).
2. `cargo test` result — total passed count, and the count of NEW tests added.
3. `cargo build` result.
4. The exact `evtx` version cargo resolved, and confirm you used the 1.98.1 PATH
   (paste `cargo --version`).
5. Any deviation from this plan, and any place where a test still fails.
