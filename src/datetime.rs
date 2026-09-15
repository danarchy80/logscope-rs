//! Shared datetime parsing for LogScope CLI and GUI.

use chrono::{DateTime, NaiveDateTime, Utc};

/// Parse a user-supplied datetime into UTC.
///
/// Tries, in order:
/// 1. RFC 3339 (e.g. `2026-08-26T10:00:00+00:00`)
/// 2. `%Y-%m-%d %H:%M:%S` (assumed UTC)
/// 3. `%Y-%m-%dT%H:%M:%S` (assumed UTC)
pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(naive.and_utc());
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(naive.and_utc());
    }
    Err(format!(
        "invalid datetime {s:?}: expected RFC 3339, \"YYYY-MM-DD HH:MM:SS\", or \"YYYY-MM-DDTHH:MM:SS\""
    ))
}
