//! Log line parser — ported from Python `logscope/core/parser.py`.
//!
//! Behavior contract:
//! - A line whose START matches one of the five timestamp patterns begins a new
//!   entry. Python `re.match` semantics are reproduced with `match_anchored`
//!   (match at position 0 only).
//! - A line with no matching leading timestamp is a continuation of the current
//!   entry (appended to `raw_lines`).
//! - Lines before the first timestamped entry are DROPPED.
//! - All timestamps are normalized to UTC at parse time.

use std::sync::LazyLock;

use chrono::{DateTime, Datelike, Local, NaiveDateTime, Utc};
use regex::Regex;

/// The five timestamp detection patterns, ported VERBATIM from Python, in
/// priority order. Compiled once; later phases reuse `TIMESTAMP_PATTERNS`
/// (e.g. normalize needs the match span to strip the timestamp prefix).
pub static TIMESTAMP_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // 1. ISO 8601 with 'Z'
        Regex::new(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)").unwrap(),
        // 2. ISO 8601 with numeric offset
        Regex::new(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[+-]\d{2}:?\d{2}))")
            .unwrap(),
        // 3. Space-separated, optional offset (optional leading space before it)
        Regex::new(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?(?:\s?[+-]\d{2}:?\d{2})?)")
            .unwrap(),
        // 4. Apache common log format
        Regex::new(r"(\d{1,2}/\w{3}/\d{4}:\d{2}:\d{2}:\d{2} [+-]\d{2}:?\d{2})").unwrap(),
        // 5. Syslog with RFC 3164 <PRI> prefix, e.g. "<34>Aug 26 10:00:00"
        Regex::new(r"(<\d+>\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})").unwrap(),
        // 6. Syslog (no year, no PRI)
        Regex::new(r"(\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})").unwrap(),
    ]
});

/// Python's strptime format list, expanded for chrono's stricter offset tokens.
///
/// chrono differences handled here:
/// - Python `%f` (fraction, with the dot) == chrono `%.f` (dot + fraction).
/// - Python `%z` accepts `+0200`, `+02:00`, and `Z`; chrono splits these:
///   `%z` accepts `+0200` only, `%:z` accepts `+02:00` only, and trailing `Z`
///   is handled by RFC 3339 parsing instead.
///
/// Each entry is tried in order; the first successful parse wins, mirroring the
/// Python try/except loop.
fn try_formats(ts_str: &str) -> Option<DateTime<Utc>> {
    // Formats ending in literal 'Z': parse as RFC 3339 (chrono's %Z does not
    // accept 'Z' as a UTC designator in parse_from_str for these shapes).
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
        return Some(dt.with_timezone(&Utc));
    }

    // Python order: "%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M:%S.%f%z"
    for base in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M:%S%.f"] {
        // chrono %z: "+0200"
        if let Ok(dt) = DateTime::parse_from_str(ts_str, &format!("{base}%z")) {
            return Some(dt.with_timezone(&Utc));
        }
        // chrono %:z: "+02:00"
        if let Ok(dt) = DateTime::parse_from_str(ts_str, &format!("{base}%:z")) {
            return Some(dt.with_timezone(&Utc));
        }
    }

    // Python order: "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M:%S.%f" (naive -> UTC)
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(ts_str, fmt) {
            return Some(naive.and_utc());
        }
    }

    // Python order: "%Y-%m-%d %H:%M:%S%z", "%Y-%m-%d %H:%M:%S.%f%z",
    //              "%Y-%m-%d %H:%M:%S %z", "%Y-%m-%d %H:%M:%S.%f %z"
    // (the " %z" variants cover the SPACE-SEP optional leading-space offset,
    //  e.g. "2026-08-26 10:00:00 +0200")
    for base in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S ",
        "%Y-%m-%d %H:%M:%S%.f ",
    ] {
        if let Ok(dt) = DateTime::parse_from_str(ts_str, &format!("{base}%z")) {
            return Some(dt.with_timezone(&Utc));
        }
        if let Ok(dt) = DateTime::parse_from_str(ts_str, &format!("{base}%:z")) {
            return Some(dt.with_timezone(&Utc));
        }
    }

    // Python: "%d/%b/%Y:%H:%M:%S %z" (Apache; abbreviated English month -> %b)
    if let Ok(dt) = DateTime::parse_from_str(ts_str, "%d/%b/%Y:%H:%M:%S %z") {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = DateTime::parse_from_str(ts_str, "%d/%b/%Y:%H:%M:%S %:z") {
        return Some(dt.with_timezone(&Utc));
    }

    None
}

/// Parse a captured timestamp string into a UTC datetime.
///
/// Syslog fallback: Python does `dt.replace(year=datetime.now().year)`.
/// YEAR-BOUNDARY LIMITATION (reproduced from Python, do not "fix"): syslog lines
/// carry no year, so we stamp the CURRENT local year. Logs from a prior calendar
/// year are mis-dated; callers filtering across year boundaries must be aware.
fn parse_timestamp_string(ts_str: &str) -> Option<DateTime<Utc>> {
    if let Some(dt) = try_formats(ts_str) {
        return Some(dt);
    }
    // RFC 3164 syslog may carry a leading `<PRI>` prefix (e.g. "<34>Aug 26
    // 10:00:00"). Strip it before the syslog no-year fallback parse.
    let syslog_ts = strip_pri(ts_str);
    // Syslog (e.g. "Aug 26 10:00:00") has no year; assume the current year.
    // chrono's NaiveDateTime::parse_from_str REQUIRES a year (Python's strptime
    // silently defaults it to 1900, then .replace(year=now.year)). So prepend
    // the current year before parsing, then convert to UTC.
    let year = Local::now().year();
    let full = format!("{year} {syslog_ts}");
    if let Ok(naive) = NaiveDateTime::parse_from_str(&full, "%Y %b %d %H:%M:%S") {
        return Some(naive.and_utc());
    }
    None
}

/// Strip a leading RFC 3164 `<PRI>` (e.g. `<34>`) if present. Returns the
/// original string unchanged when there is no PRI prefix.
fn strip_pri(s: &str) -> &str {
    if s.starts_with('<') {
        if let Some(idx) = s.find('>') {
            return &s[idx + 1..];
        }
    }
    s
}

/// Parse a single log line, returning its timestamp in UTC if the line STARTS
/// with a recognized timestamp (Python `re.match` semantics).
pub fn parse_line(line: &str) -> Option<DateTime<Utc>> {
    for pattern in TIMESTAMP_PATTERNS.iter() {
        // Python `re.match` = match anchored at the START of the line. The
        // `regex` crate has no match_anchored, so we assert the overall match
        // begins at position 0 to reproduce that semantics.
        if let Some(caps) = pattern.captures(line) {
            if caps.get(0).map(|m| m.start()) != Some(0) {
                continue;
            }
            let ts_str: &str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Some(dt) = parse_timestamp_string(ts_str) {
                return Some(dt);
            }
        }
    }
    None
}

/// Parse a file's lines into a `LogSource`, grouping continuation lines under
/// the preceding timestamped entry. Lines before the first timestamp are
/// dropped. Mirrors Python `parse_file(source_name, lines)`.
pub fn parse_file(source_name: &str, lines: &[String]) -> crate::models::LogSource {
    use crate::models::{Level, LogEntry, LogSource};
    use crate::core::level::detect_level;

    let mut source = LogSource { name: source_name.to_string(), entries: Vec::new() };
    let mut current_entry: Option<LogEntry> = None;

    for (i, line) in lines.iter().enumerate() {
        if let Some(ts) = parse_line(line) {
            if let Some(mut entry) = current_entry.take() {
                entry.level = detect_level(&entry.raw_lines);
                source.entries.push(entry);
            }
            current_entry = Some(LogEntry {
                timestamp: ts,
                raw_lines: vec![line.clone()],
                source: source_name.to_string(),
                original_line_number: i,
                level: Level::Unknown,
            });
        } else if let Some(entry) = current_entry.as_mut() {
            entry.raw_lines.push(line.clone());
        }
    }
    if let Some(mut entry) = current_entry {
        entry.level = detect_level(&entry.raw_lines);
        source.entries.push(entry);
    }
    source
}
