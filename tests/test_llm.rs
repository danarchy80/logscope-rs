//! Tests for `logscope::core::llm` — no network calls.

use chrono::{TimeZone, Utc};
use logscope::core::llm::{build_prompt, parse_analysis, AnalysisResult, KeyEvent};
use logscope::core::sanitize::{sanitize_entries, SanitizeConfig};
use logscope::models::{Level, LogEntry};

fn make_entry(msg: &str) -> LogEntry {
    LogEntry {
        timestamp: Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap(),
        raw_lines: vec![msg.to_string()],
        source: "test.log".to_string(),
        original_line_number: 0,
        level: Level::Info,
    }
}

// ---- 1. build_prompt ----

#[test]
fn build_prompt_contains_first_and_last_entry() {
    let mut entries = vec![make_entry("FIRST_MARKER_entry")];
    for i in 0..50 {
        entries.push(make_entry(&format!("middle {i}")));
    }
    entries.push(make_entry("LAST_MARKER_entry"));

    let prompt = build_prompt(&entries, 0);
    assert!(prompt.contains("FIRST_MARKER_entry"), "first entry missing");
    assert!(prompt.contains("LAST_MARKER_entry"), "last entry missing");
    // Entry line format: {ts} [{LEVEL}] ({source}) {msg}
    assert!(prompt.contains("[INFO] (test.log) FIRST_MARKER_entry"));
}

#[test]
fn build_prompt_respects_budget_with_many_entries() {
    let entries: Vec<LogEntry> = (0..5000)
        .map(|i| make_entry(&format!("entry number {i} with some filler text to add length")))
        .collect();

    let max_chars = 4_000;
    let prompt = build_prompt(&entries, max_chars);
    assert!(
        prompt.len() <= max_chars + 1000,
        "prompt len {} exceeds budget {}",
        prompt.len(),
        max_chars + 1000
    );
    assert!(prompt.contains("TRUNCATED"), "truncation marker missing");
    // First and last entries still present after truncation.
    assert!(prompt.contains("entry number 0 "));
    assert!(prompt.contains("entry number 4999 "));
}

// ---- 2. parse_analysis ----

#[test]
fn parse_analysis_plain_json() {
    let text = r#"{"summary":"all good","key_events":[{"timestamp":"2026-10-20 12:00:00","level":"ERROR","title":"boom","detail":"it boomed","source":"app.log"}],"recommendations":["fix it"]}"#;
    let res = parse_analysis(text).unwrap();
    assert_eq!(res.summary, "all good");
    assert_eq!(res.key_events.len(), 1);
    assert_eq!(res.key_events[0].level, "ERROR");
    assert_eq!(res.recommendations, vec!["fix it".to_string()]);
}

#[test]
fn parse_analysis_fenced_json_block() {
    let text = "```json\n{\"summary\":\"s\",\"key_events\":[],\"recommendations\":[]}\n```";
    let res = parse_analysis(text).unwrap();
    assert_eq!(res.summary, "s");
}

#[test]
fn parse_analysis_fenced_plain_block() {
    let text = "```\n{\"summary\":\"s2\",\"key_events\":[],\"recommendations\":[]}\n```";
    let res = parse_analysis(text).unwrap();
    assert_eq!(res.summary, "s2");
}

#[test]
fn parse_analysis_wrapped_in_prose() {
    let text = "Sure! Here is the analysis:\n{\"summary\":\"wrapped\",\"key_events\":[],\"recommendations\":[]}\nHope that helps!";
    let res = parse_analysis(text).unwrap();
    assert_eq!(res.summary, "wrapped");
}

#[test]
fn parse_analysis_missing_optional_fields() {
    let res = parse_analysis(r#"{"summary":"only summary"}"#).unwrap();
    assert_eq!(res.summary, "only summary");
    assert_eq!(res.key_events, Vec::<KeyEvent>::new());
    assert_eq!(res.recommendations, Vec::<String>::new());

    let empty_obj = parse_analysis("{}").unwrap();
    assert_eq!(empty_obj, AnalysisResult::default());

    // KeyEvent with missing optional string fields => ""
    let res = parse_analysis(
        r#"{"key_events":[{"level":"INFO"}],"summary":"x","recommendations":[]}"#,
    )
    .unwrap();
    assert_eq!(res.key_events[0].timestamp, "");
    assert_eq!(res.key_events[0].title, "");
}

#[test]
fn parse_analysis_garbage_is_err() {
    assert!(parse_analysis("no braces here at all").is_err());
    assert!(parse_analysis("{ not valid json ,,, }").is_err());
    assert!(parse_analysis("").is_err());
}

// ---- 3. Redaction integration ----

#[test]
fn sanitized_entries_have_redacted_ip_in_prompt() {
    let mut entries = vec![make_entry("connection from 192.168.50.77 refused")];
    let cfg = SanitizeConfig {
        redact_ips: true,
        ..Default::default()
    };
    sanitize_entries(&mut entries, &cfg);

    let prompt = build_prompt(&entries, 0);
    assert!(prompt.contains("[REDACTED_IP]"), "IP token missing: {prompt}");
    assert!(
        !prompt.contains("192.168.50.77"),
        "raw IP leaked into prompt"
    );
}

// ---- 4. KeyEvent serde round-trip ----

#[test]
fn key_event_serde_round_trip() {
    let event = KeyEvent {
        timestamp: "2026-10-20 12:00:00.123".to_string(),
        level: "CRITICAL".to_string(),
        title: "disk full".to_string(),
        detail: "/var at 100%".to_string(),
        source: "syslog".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: KeyEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}
