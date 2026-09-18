//! Parser tests — ported 1:1 from the Python pytest suite.

use chrono::{DateTime, Datelike, Timelike, Utc};
use logscope::core::parser::{parse_file, parse_line};

// Sample files inlined from tests/samples/*.log (Python repo).
const SIMPLE_LOG: &str = "\
2026-08-26 10:00:00 INFO Application started
2026-08-26 10:01:00 INFO Loading config
2026-08-26 10:02:30 WARN Connection timeout
2026-08-26 10:05:00 ERROR Failed to connect
2026-08-26 10:10:00 INFO Retrying
2026-08-26 10:15:00 INFO Connected successfully";

const MULTILINE_LOG: &str = "\
2026-08-26 10:00:00 ERROR Something went wrong
Traceback (most recent call last):
  File \"app.py\", line 42, in <module>
    main()
  File \"app.py\", line 18, in main
    raise RuntimeError(\"boom\")
RuntimeError: boom
2026-08-26 10:01:00 INFO Recovered";

const NO_TIMESTAMPS_LOG: &str = "\
This line has no timestamp
Neither does this one
2026-08-26 10:00:00 This one does
But this continuation line does not";

fn lines(s: &str) -> Vec<String> {
    s.lines().map(|l| l.to_string()).collect()
}

#[test]
fn test_parse_iso_z() {
    let ts = parse_line("2026-08-26T10:00:00Z INFO Something").expect("should parse ISO-Z");
    assert_eq!(ts.year(), 2026);
    assert_eq!(ts.month(), 8);
    assert_eq!(ts.day(), 26);
    assert_eq!(ts.hour(), 10);
    assert_eq!(ts.timezone(), Utc);
}

#[test]
fn test_parse_space_separated() {
    let ts = parse_line("2026-08-26 10:00:00 INFO Something").expect("should parse space-separated");
    assert_eq!(ts.hour(), 10);
}

#[test]
fn test_parse_with_millis() {
    let ts = parse_line("2026-08-26 10:00:00.123 INFO").expect("should parse millis");
    assert_eq!(ts.timestamp_subsec_micros(), 123000);
}

#[test]
fn test_parse_apache() {
    let ts = parse_line("26/Aug/2026:10:00:00 +0000 INFO").expect("should parse Apache");
    assert_eq!(ts.day(), 26);
}

#[test]
fn test_parse_syslog_no_year() {
    // Syslog has no year; Python assumes current year. Verify it parses (day/hour)
    // rather than returning None — the year is current-year-stamped.
    let ts = parse_line("Aug 26 10:00:00 INFO").expect("should parse syslog");
    assert_eq!(ts.month(), 8);
    assert_eq!(ts.day(), 26);
    assert_eq!(ts.hour(), 10);
}

#[test]
fn test_parse_syslog_with_pri() {
    // RFC 3164 <PRI> prefix (e.g. "<34>") precedes the syslog timestamp; the
    // timestamp is NOT at position 0, but must still parse.
    let ts = parse_line("<34>Aug 26 10:00:00 myhost sshd: failed").expect("should parse PRI syslog");
    assert_eq!(ts.month(), 8);
    assert_eq!(ts.day(), 26);
    assert_eq!(ts.hour(), 10);
}

#[test]
fn test_no_timestamp_line() {
    assert!(parse_line("This line has no timestamp").is_none());
}

#[test]
fn test_continuation_line() {
    assert!(parse_line("  File \"app.py\", line 42").is_none());
}

#[test]
fn test_parse_file_simple() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    assert_eq!(source.entries.len(), 6);
    assert_eq!(source.entries[0].timestamp.hour(), 10);
    assert_eq!(source.entries[0].source, "simple.log");
    assert_eq!(source.entries[0].original_line_number, 0);
}

#[test]
fn test_parse_file_multiline() {
    let source = parse_file("multiline.log", &lines(MULTILINE_LOG));
    assert_eq!(source.entries.len(), 2);
    assert_eq!(source.entries[0].raw_lines.len(), 7);
    assert_eq!(source.entries[1].raw_lines.len(), 1);
}

#[test]
fn test_parse_file_no_timestamps() {
    let source = parse_file("no_timestamps.log", &lines(NO_TIMESTAMPS_LOG));
    // Leading non-timestamped lines are dropped; one entry with its continuation.
    assert_eq!(source.entries.len(), 1);
    assert_eq!(source.entries[0].timestamp.hour(), 10);
    assert_eq!(source.entries[0].raw_lines.len(), 2);
}

#[test]
fn test_space_sep_with_offset_converts_to_utc() {
    // "10:00:00 +0200" wall-clock == 08:00 UTC.
    let ts: DateTime<Utc> =
        parse_line("2026-08-26 10:00:00 +0200 INFO with offset").expect("should parse offset");
    assert_eq!(ts.hour(), 8);
}
