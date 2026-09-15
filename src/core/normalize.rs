//! Timestamp/entry normalization — ported from Python `logscope/core/normalize.py`.
//!
//! Byte-parity contract:
//! - `normalize_timestamp`: UTC, ISO-8601 with literal `Z` suffix. Fractional
//!   seconds are emitted ONLY when microseconds > 0, always as exactly 6 digits
//!   (Python `%f` zero-padding == chrono `%.6f`).
//! - `normalize_entry`: the leading timestamp of the first raw line is REPLACED
//!   by the normalized timestamp + `[source]` tag with NO space between `]` and
//!   the remainder (the remainder already carries the original separator). If
//!   the first line has no leading timestamp, a single space is inserted.
//!   Continuation lines are appended verbatim, joined with `\n`.

use chrono::{DateTime, Utc};

use crate::core::parser::TIMESTAMP_PATTERNS;
use crate::models::LogEntry;

/// Normalize a datetime to `YYYY-MM-DDTHH:MM:SSZ` (or with `.ffffff` when
/// microseconds are non-zero). Input is already UTC (`DateTime<Utc>`), so
/// Python's `astimezone(utc)` step is implicit.
pub fn normalize_timestamp(dt: &DateTime<Utc>) -> String {
    if dt.timestamp_subsec_micros() != 0 {
        format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S%.6f"))
    } else {
        format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S"))
    }
}

/// Normalize one entry into its export block form:
/// `<utc-ts> [<source>]<rest-of-first-line>\n<continuation lines...>`.
pub fn normalize_entry(entry: &LogEntry) -> String {
    let ts_str = normalize_timestamp(&entry.timestamp);
    let first_line = &entry.raw_lines[0];

    // Python: first pattern whose `.match()` (anchored at start) succeeds;
    // slice `first_line[ts_match.end():]` from the END of the WHOLE match
    // (group 0), including any trailing offset like "+0200" and the optional
    // leading space of a space-separated offset.
    let mut ts_match_end: Option<usize> = None;
    for pattern in TIMESTAMP_PATTERNS.iter() {
        if let Some(caps) = pattern.captures(first_line) {
            if caps.get(0).map(|m| m.start()) == Some(0) {
                ts_match_end = Some(caps.get(0).unwrap().end());
                break;
            }
        }
    }

    let mut lines: Vec<String> = Vec::with_capacity(entry.raw_lines.len());
    match ts_match_end {
        Some(end) => lines.push(format!("{ts_str} [{}]{}", entry.source, &first_line[end..])),
        None => lines.push(format!("{ts_str} [{}] {}", entry.source, first_line)),
    }
    lines.extend(entry.raw_lines[1..].iter().cloned());
    lines.join("\n")
}
