//! Core data model, ported from Python `logscope/models.py`.
//!
//! Python's naive/aware datetime distinction collapses here: every timestamp is
//! normalized to UTC at parse time (`DateTime<Utc>`).

use chrono::{DateTime, Utc};

/// A single log entry, possibly spanning multiple lines.
///
/// `original_line_number` is the 0-based index of the entry's timestamp line
/// in its source file. It is currently populated but not surfaced in export;
/// it exists to support future "jump to source" or diagnostics features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub raw_lines: Vec<String>,
    pub source: String,
    pub original_line_number: usize,
}

/// A named collection of parsed log entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSource {
    pub name: String,
    pub entries: Vec<LogEntry>,
}
