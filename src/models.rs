//! Core data model, ported from Python `logscope/models.py`.
//!
//! Python's naive/aware datetime distinction collapses here: every timestamp is
//! normalized to UTC at parse time (`DateTime<Utc>`).

use chrono::{DateTime, Utc};

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
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
            Level::Critical => "CRITICAL",
            Level::Unknown => "UNKNOWN",
        }
    }

    /// Case-insensitive parse. Accepts "critical"|"fatal", "error"|"err",
    /// "warn"|"warning", "info", "debug", "trace". Returns None for anything else.
    pub fn parse(s: &str) -> Option<Level> {
        let s = s.to_lowercase();
        match s.as_str() {
            "critical" | "fatal" => Some(Level::Critical),
            "error" | "err" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warning),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            "trace" => Some(Level::Trace),
            _ => None,
        }
    }
}

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
    pub level: Level,
}

/// A named collection of parsed log entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSource {
    pub name: String,
    pub entries: Vec<LogEntry>,
}
