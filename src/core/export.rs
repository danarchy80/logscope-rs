//! Entry formatting/export — ported from Python `logscope/core/export.py`.
//!
//! Behavior contract:
//! - Empty input formats to `""` (but `export_entries` still writes `"\n"`).
//! - Entries are sorted by timestamp ASC with a STABLE sort (Python `sorted()`
//!   is stable; `Vec::sort_by` is stable — do NOT use `sort_unstable_by`).
//! - Blocks are joined with a blank line (`"\n\n"`).

use std::path::Path;

use crate::core::ingest::IngestError;
use crate::core::normalize::normalize_entry;
use crate::models::LogEntry;

/// Format entries into a single export string: stable-sorted by timestamp,
/// each normalized, joined by `"\n\n"`. Empty slice → `""`.
pub fn format_entries(entries: &[LogEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<&LogEntry> = entries.iter().collect();
    // Stable sort mirrors Python's `sorted(..., key=timestamp)`.
    sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    let blocks: Vec<String> = sorted.iter().map(|entry| normalize_entry(entry)).collect();
    blocks.join("\n\n")
}

/// Write `format_entries(entries)` plus a trailing newline to `output_path`.
/// An empty entry list still writes a single `"\n"` (Python parity).
pub fn export_entries(entries: &[LogEntry], output_path: &Path) -> Result<(), IngestError> {
    let content = format_entries(entries);
    std::fs::write(output_path, format!("{content}\n").as_bytes())?;
    Ok(())
}
