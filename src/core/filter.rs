//! Time-range filtering — ported from Python `logscope/core/filter.py`.
//!
//! Behavior contract:
//! - Inclusive on BOTH ends: `start <= ts <= end`.
//! - Python's naive-datetime coercion (`_ensure_aware`) is a no-op here: all
//!   timestamps are `DateTime<Utc>` after parse time.

use chrono::{DateTime, Utc};

use crate::models::{LogEntry, LogSource};

/// Return clones of all entries across `sources` whose timestamp falls inside
/// the inclusive range `[start, end]`, preserving source/entry order.
pub fn filter_entries(
    sources: &[LogSource],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<LogEntry> {
    sources
        .iter()
        .flat_map(|source| source.entries.iter())
        .filter(|entry| start <= entry.timestamp && entry.timestamp <= end)
        .cloned()
        .collect()
}
