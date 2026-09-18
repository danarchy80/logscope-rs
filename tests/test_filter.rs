//! Integration tests for `logscope::core::filter` — ported from
//! `logscope/tests/test_filter.py` (6 cases). Sample bytes inlined verbatim
//! from the Python repo's tests/samples/simple.log and multiline.log.

use chrono::{DateTime, TimeZone, Utc};

use logscope::core::filter::filter_entries;
use logscope::core::parser::parse_file;

const SIMPLE_LOG: &str = "\
2026-08-26 10:00:00 INFO Application started
2026-08-26 10:01:00 INFO Loading config
2026-08-26 10:02:30 WARN Connection timeout
2026-08-26 10:05:00 ERROR Failed to connect
2026-08-26 10:10:00 INFO Retrying
2026-08-26 10:15:00 INFO Connected successfully
";

const MULTILINE_LOG: &str = "\
2026-08-26 10:00:00 ERROR Something went wrong
Traceback (most recent call last):
  File \"app.py\", line 42, in <module>
    main()
  File \"app.py\", line 18, in main
    raise RuntimeError(\"boom\")
RuntimeError: boom
2026-08-26 10:01:00 INFO Recovered
";

fn lines(text: &str) -> Vec<String> {
    text.lines().map(String::from).collect()
}

fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s).single().unwrap()
}

/// 1. test_filter_basic
#[test]
fn filter_basic() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let result = filter_entries(&[source], ts(2026, 8, 26, 10, 1, 0), ts(2026, 8, 26, 10, 5, 0));
    assert_eq!(result.len(), 3);
}

/// 2. test_filter_inclusive_boundaries
#[test]
fn filter_inclusive_boundaries() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let t = ts(2026, 8, 26, 10, 0, 0);
    let result = filter_entries(&[source], t, t);
    assert_eq!(result.len(), 1);
}

/// 3. test_filter_no_matches
#[test]
fn filter_no_matches() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let result = filter_entries(&[source], ts(2026, 8, 26, 11, 0, 0), ts(2026, 8, 26, 12, 0, 0));
    assert!(result.is_empty());
}

/// 4. test_filter_multiline_included
#[test]
fn filter_multiline_included() {
    let source = parse_file("multiline.log", &lines(MULTILINE_LOG));
    let result = filter_entries(&[source], ts(2026, 8, 26, 10, 0, 0), ts(2026, 8, 26, 10, 0, 59));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].raw_lines.len(), 7);
}

/// 5. test_filter_multiple_sources
#[test]
fn filter_multiple_sources() {
    let sources = vec![
        parse_file("simple.log", &lines(SIMPLE_LOG)),
        parse_file("multiline.log", &lines(MULTILINE_LOG)),
    ];
    let result = filter_entries(&sources, ts(2026, 8, 26, 10, 0, 0), ts(2026, 8, 26, 10, 0, 59));
    assert_eq!(result.len(), 2);
}

/// 6. test_filter_timezone_aware — all datetimes are UTC-aware in the Rust port,
/// so this is the same inclusive-range check as test_filter_basic.
#[test]
fn filter_timezone_aware() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let result = filter_entries(&[source], ts(2026, 8, 26, 10, 0, 0), ts(2026, 8, 26, 10, 5, 0));
    assert!(result.len() >= 3);
}

#[test]
fn filter_with_options_level() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let opts = logscope::core::filter::FilterOptions {
        start: ts(2026, 8, 26, 10, 0, 0),
        end: ts(2026, 8, 26, 10, 15, 0),
        levels: Some(vec![logscope::models::Level::Error]),
        sources: None,
        search: None,
    };
    let result = logscope::core::filter::filter_entries_with_options(&[source], &opts);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].level, logscope::models::Level::Error);
}

#[test]
fn filter_with_options_source() {
    let mut source1 = parse_file("auth.log", &lines(SIMPLE_LOG));
    let mut source2 = parse_file("app.log", &lines(SIMPLE_LOG));
    source1.name = "auth.log".to_string();
    for e in &mut source1.entries { e.source = "auth.log".to_string(); }
    source2.name = "app.log".to_string();
    for e in &mut source2.entries { e.source = "app.log".to_string(); }
    
    let opts = logscope::core::filter::FilterOptions {
        start: ts(2026, 8, 26, 10, 0, 0),
        end: ts(2026, 8, 26, 10, 15, 0),
        levels: None,
        sources: Some(vec!["auth".to_string()]),
        search: None,
    };
    let result = logscope::core::filter::filter_entries_with_options(&[source1, source2], &opts);
    assert_eq!(result.len(), 6);
    for e in result {
        assert_eq!(e.source, "auth.log");
    }
}

#[test]
fn filter_with_options_search_match() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let opts = logscope::core::filter::FilterOptions {
        start: ts(2026, 8, 26, 10, 0, 0),
        end: ts(2026, 8, 26, 10, 15, 0),
        levels: None,
        sources: None,
        search: Some("timeout".to_string()),
    };
    let result = logscope::core::filter::filter_entries_with_options(&[source], &opts);
    assert_eq!(result.len(), 1);
    assert!(result[0].raw_lines.join("\n").to_lowercase().contains("timeout"));
}

#[test]
fn filter_with_options_search_no_match() {
    let source = parse_file("simple.log", &lines(SIMPLE_LOG));
    let opts = logscope::core::filter::FilterOptions {
        start: ts(2026, 8, 26, 10, 0, 0),
        end: ts(2026, 8, 26, 10, 15, 0),
        levels: None,
        sources: None,
        search: Some("nomatchzzz".to_string()),
    };
    let result = logscope::core::filter::filter_entries_with_options(&[source], &opts);
    assert!(result.is_empty());
}
