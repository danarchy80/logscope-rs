# LogScope → Rust Migration Plan

> Architect: high-cap model (Nous). Coder: EVO-X3 (halogen-qwen3.8-flash-next).
> QA: high-cap model. Source of truth for behavior: the existing Python
> implementation at /home/danarchy/workspace/logscope (49 passing pytest tests)
> and its README output-format contract.

## Objective

Rewrite LogScope's **core library** in Rust, preserving behavior exactly.
Frontend (CLI vs egui GUI) is deferred and independent of this work.

Target dir: `/home/danarchy/workspace/logscope-rs/` (a NEW sibling crate; do not
touch the Python repo).

## Scope of THIS pass (frontend-independent core)

1. `models` — `LogEntry`, `LogSource` structs.
2. `parser` — timestamp detection + multi-line grouping.
3. `ingest` — file/folder/zip/tar/gzip/bzip2 extraction.
4. `filter` — inclusive time-range filtering.
5. `normalize` — ISO 8601 UTC normalization.
6. `export` — chronological sort + source-prefixed merge.
7. `pipeline` — orchestration.
8. Unit tests porting the 49 pytest assertions 1:1.

## Behavioral Spec (must match Python exactly)

### Timestamp formats detected (in priority order)
1. ISO 8601 + Z: `YYYY-MM-DDTHH:MM:SS(.f)?Z`
2. ISO 8601 + numeric offset: `YYYY-MM-DDTHH:MM:SS(.f)?(+|-)HH(:)?MM`
3. Space-separated: `YYYY-MM-DD HH:MM:SS(.f)?` + optional `\s?(+|-)HH(:)?MM`
4. Apache common log: `DD/Mon/YYYY:HH:MM:SS (+|-)HH(:)?MM`
5. Syslog: `Mon DD HH:MM:SS` (no year → assume current year)

### Parsing rules
- A line whose start matches a timestamp pattern begins a new entry.
- A line with no matching leading timestamp is a continuation of the current
  entry (appended to `raw_lines`).
- Lines before the first timestamped entry are DROPPED.
- `original_line_number` is the 0-based index of the entry's first line.

### Filtering
- Inclusive on both boundaries: `start <= ts <= end`.
- Naive (no-tz) datetimes are treated as UTC.
- Aware datetimes are compared after conversion to UTC. In Rust, store
  everything as `DateTime<Utc>` immediately at parse time.

### Normalization → output contract
- Timestamp normalized to `YYYY-MM-DDTHH:MM:SS(.ffffff)?Z` (UTC).
- Line 1 of each entry: `{iso_ts} [{source}]{rest_of_line}` where
  `rest_of_line` is the original first line with the matched timestamp prefix
  stripped (keep whatever separator followed it, incl. leading space).
  - If BOTH an offset and a `Z` mismatch happen, the stripped offset is what the
    Python re-match removes — port the exact `_TIMESTAMP_PATTERNS` match-end
    slicing behavior.
- Continuation lines are emitted verbatim (no `[source]` prefix).
- Entries separated by a blank line; sorted chronologically (UTC); stable sort
  for equal timestamps (preserve ingestion order).
- Empty result → empty output (Python writes `"\n"` via `content + "\n"`; see
  note).

### Ingestion
- Single file: read UTF-8 with invalid bytes replaced (`.replace` → Rust
  `String::from_utf8_lossy`).
- `.gz` / `.bz2` single file (NOT tar): decompress then read.
- `.zip`: extract every non-directory member, sorted by member name.
- `.tar`/`.tar.gz`/`.tar.bz2`/`.tgz`/`.tbz2`: detect tar, extract file members
  sorted by name.
- Folder: `os.walk` (recursive), files sorted per directory; nested archives
  are expanded; other files read as text.
- Nonexistent path → empty source list (do NOT error).
- Corrupt/unreadable file at the top level → ERROR (raises / returns Err), per
  the pass-2 fix.

## Known Python quirks to reproduce (do NOT "fix")
- Syslog assumes current year (comment it, keep behavior).
- Malformed `YYYY-MM-DD HH:MM:SSZ` (space + trailing Z) leaves a stray `Z` in
  the body — reproduce the same regex behavior, do not special-case it.

## Rust crate decisions
- `chrono` for datetimes (`DateTime<Utc>`), `NaiveDate`+`NaiveTime` for nosy
  parsing.
- `regex` for the five patterns (use `Regex` and `captures` at position 0; port
  the exact Python patterns including the `\s?` optional offset from pass 1).
- `zip`, `tar`, `flate2`/`gzip`, `bzip2` for archives.
- `anyhow` for error handling in the lib boundary (or plain `Result<_, Box<dyn
  Error>>`; pick one and be consistent).
- Library crate `logscope` + a binary crate `logscope-cli` (binary stub only —
  actual CLI args come in a later pass).

## Delivery order (each phase = one coder dispatch)
1. Cargo scaffold + `models` + `parser` + parser tests.
2. `ingest` + ingest tests.
3. `filter` + `normalize` + `export` + their tests.
4. `pipeline` + e2e tests.
5. QA: `cargo test` green; compare against Python on the shared sample logs.

## Verification (QA gate, high-cap model)
- `cargo test` passes (port count ≥ 49 assertions).
- Run the Rust pipeline against `tests/samples/*.log` and diff byte-for-byte
  against the Python output for the same inputs/ranges. Byte-identical output
  is the acceptance bar. (Note: the Python `export_entries` writes
  `content + "\n"`; reproduce that trailing newline exactly.)
- Rust 1.93 is installed; do NOT add a GUI dependency in this pass.

## Out of scope (later passes)
- CLI argument parsing (clap), full CLI UX.
- GUI (egui/iced) if chosen.
- Windows exe build (GitHub Actions runner), replacing the PyInstaller flow.
- Deleting the Python repo (only after byte-parity is proven).