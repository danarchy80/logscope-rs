//! Integration tests for `logscope::core::normalize` — ported from
//! `logscope/tests/test_normalize.py` (6 cases).

use chrono::{DateTime, FixedOffset, TimeZone, Timelike, Utc};

use logscope::core::normalize::{normalize_entry, normalize_timestamp};
use logscope::models::LogEntry;

fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, us: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
        .single()
        .unwrap()
        .with_nanosecond(us * 1000)
        .unwrap()
}

/// 1. test_normalize_utc
#[test]
fn normalize_utc() {
    let dt = utc(2026, 8, 26, 10, 0, 0, 0);
    assert_eq!(normalize_timestamp(&dt), "2026-08-26T10:00:00Z");
}

/// 2. test_normalize_with_microseconds
#[test]
fn normalize_with_microseconds() {
    let dt = utc(2026, 8, 26, 10, 0, 0, 123000);
    assert_eq!(normalize_timestamp(&dt), "2026-08-26T10:00:00.123000Z");
}

/// 3. test_normalize_naive_assumed_utc — naive datetimes collapse to UTC at
/// the type level in the Rust port; same expected output.
#[test]
fn normalize_naive_assumed_utc() {
    let dt = utc(2026, 8, 26, 10, 0, 0, 0);
    assert_eq!(normalize_timestamp(&dt), "2026-08-26T10:00:00Z");
}

/// 4. test_normalize_with_offset — 12:00 in +02:00 == 10:00 UTC.
#[test]
fn normalize_with_offset() {
    let plus2 = FixedOffset::east_opt(2 * 3600).unwrap();
    let dt: DateTime<FixedOffset> = plus2
        .with_ymd_and_hms(2026, 8, 26, 12, 0, 0)
        .single()
        .unwrap();
    let dt_utc: DateTime<Utc> = dt.into();
    assert_eq!(normalize_timestamp(&dt_utc), "2026-08-26T10:00:00Z");
}

/// 5. test_normalize_entry_single_line
#[test]
fn normalize_entry_single_line() {
    let entry = LogEntry {
        timestamp: utc(2026, 8, 26, 10, 0, 0, 0),
        raw_lines: vec!["2026-08-26 10:00:00 INFO Application started".to_string()],
        source: "app.log".to_string(),
        original_line_number: 0,
    };
    let result = normalize_entry(&entry);
    assert!(result.starts_with("2026-08-26T10:00:00Z"));
    assert!(result.contains("INFO Application started"));
    assert!(result.contains("[app.log]"));
}

/// 6. test_normalize_entry_multiline
#[test]
fn normalize_entry_multiline() {
    let entry = LogEntry {
        timestamp: utc(2026, 8, 26, 10, 0, 0, 0),
        raw_lines: vec![
            "2026-08-26 10:00:00 ERROR Something went wrong".to_string(),
            "Traceback (most recent call last):".to_string(),
            "  File \"app.py\", line 42".to_string(),
            "RuntimeError: boom".to_string(),
        ],
        source: "app.log".to_string(),
        original_line_number: 0,
    };
    let result = normalize_entry(&entry);
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines[0].starts_with("2026-08-26T10:00:00Z"));
    assert!(lines[0].contains("[app.log]"));
    assert_eq!(lines[1], "Traceback (most recent call last):");
    assert_eq!(lines[2], "  File \"app.py\", line 42");
    assert_eq!(lines[3], "RuntimeError: boom");
}
