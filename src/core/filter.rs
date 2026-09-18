//! Time-range filtering — ported from Python `logscope/core/filter.py`.
//!
//! Behavior contract:
//! - Inclusive on BOTH ends: `start <= ts <= end`.
//! - Python's naive-datetime coercion (`_ensure_aware`) is a no-op here: all
//!   timestamps are `DateTime<Utc>` after parse time.

use chrono::{DateTime, Utc};

use crate::models::{Level, LogEntry, LogSource};

#[derive(Debug, Clone)]
pub struct FilterOptions {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub levels: Option<Vec<Level>>,    // None = all levels
    pub sources: Option<Vec<String>>,  // None = all sources; else case-insensitive substring
    /// case-insensitive substring over the entry's joined raw lines; None = no search
    pub search: Option<String>,
}

pub fn filter_entries_with_options(sources: &[LogSource], opts: &FilterOptions) -> Vec<LogEntry> {
    sources
        .iter()
        .flat_map(|source| source.entries.iter())
        .filter(|entry| {
            if entry.timestamp < opts.start || entry.timestamp > opts.end {
                return false;
            }
            if let Some(ref levels) = opts.levels {
                if !levels.contains(&entry.level) {
                    return false;
                }
            }
            if let Some(ref patterns) = opts.sources {
                let entry_source_lower = entry.source.to_lowercase();
                if !patterns.iter().any(|p| entry_source_lower.contains(&p.to_lowercase())) {
                    return false;
                }
            }
            if let Some(ref needle) = opts.search {
                let text = entry.raw_lines.join("\n").to_lowercase();
                if !text.contains(&needle.to_lowercase()) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Return clones of all entries across `sources` whose timestamp falls inside
/// the inclusive range `[start, end]`, preserving source/entry order.
pub fn filter_entries(
    sources: &[LogSource],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<LogEntry> {
    filter_entries_with_options(
        sources,
        &FilterOptions {
            start,
            end,
            levels: None,
            sources: None,
            search: None,
        },
    )
}
