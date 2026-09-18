//! Integration tests for `logscope::core::export` — ported from
//! `logscope/tests/test_export.py` (7 cases).

use std::fs;

use chrono::{DateTime, TimeZone, Utc};
use tempfile::TempDir;

use logscope::core::export::{export_entries, format_entries};
use logscope::models::LogEntry;

fn utc(h: u32, mi: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 26, h, mi, 0).single().unwrap()
}

fn entry(h: u32, mi: u32, raw: &str, source: &str) -> LogEntry {
    LogEntry {
        timestamp: utc(h, mi),
        raw_lines: vec![raw.to_string()],
        source: source.to_string(),
        original_line_number: 0,
        level: logscope::models::Level::Unknown,
    }
}

/// 1. test_format_single_entry
#[test]
fn format_single_entry() {
    let result = format_entries(&[entry(10, 0, "2026-08-26 10:00:00 INFO Hello", "app.log")]);
    assert!(result.contains("2026-08-26T10:00:00Z [app.log] INFO Hello"));
}

/// 2. test_format_sorted_chronologically
#[test]
fn format_sorted_chronologically() {
    let entry1 = entry(10, 5, "2026-08-26 10:05:00 INFO Later", "b.log");
    let entry2 = entry(10, 0, "2026-08-26 10:00:00 INFO Earlier", "a.log");
    let result = format_entries(&[entry1, entry2]);
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines[0].starts_with("2026-08-26T10:00:00Z"));
    assert!(lines[2].starts_with("2026-08-26T10:05:00Z"));
}

/// 3. test_format_multiple_sources
#[test]
fn format_multiple_sources() {
    let entry1 = entry(10, 0, "2026-08-26 10:00:00 INFO From A", "a.log");
    let entry2 = entry(10, 0, "2026-08-26 10:00:00 INFO From B", "b.log");
    let result = format_entries(&[entry1, entry2]);
    assert!(result.contains("[a.log]") && result.contains("[b.log]"));
}

/// 4. test_format_multiline_entry
#[test]
fn format_multiline_entry() {
    let e = LogEntry {
        timestamp: utc(10, 0),
        raw_lines: vec![
            "2026-08-26 10:00:00 ERROR Boom".to_string(),
            "Traceback:".to_string(),
            "RuntimeError: oof".to_string(),
        ],
        source: "app.log".to_string(),
        original_line_number: 0,
        level: logscope::models::Level::Unknown,
    };
    let result = format_entries(&[e]);
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines[0].starts_with("2026-08-26T10:00:00Z [app.log]"));
    assert_eq!(lines[1], "Traceback:");
    assert_eq!(lines[2], "RuntimeError: oof");
}

/// 5. test_format_empty
#[test]
fn format_empty() {
    assert_eq!(format_entries(&[]), "");
}

/// 6. test_export_to_file
#[test]
fn export_to_file() {
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("unified.log");
    export_entries(&[entry(10, 0, "2026-08-26 10:00:00 INFO Hello", "app.log")], &out).unwrap();
    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("2026-08-26T10:00:00Z") && content.contains("[app.log]"));
}

/// 7. test_export_separator_between_entries
#[test]
fn export_separator_between_entries() {
    let entry1 = entry(10, 0, "2026-08-26 10:00:00 INFO First", "a.log");
    let entry2 = entry(10, 1, "2026-08-26 10:01:00 INFO Second", "b.log");
    let result = format_entries(&[entry1, entry2]);
    assert!(result.contains("\n\n"));
}
