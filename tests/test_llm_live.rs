//! Live end-to-end LLM analysis test — requires a reachable OpenAI-compatible
//! endpoint. Opt-in: run with `cargo test --test test_llm_live -- --ignored`.
//!
//! Override the endpoint with env vars LLM_BASE_URL / LLM_API_KEY / LLM_MODEL.

use chrono::{TimeZone, Utc};
use logscope::core::llm::{self, LlmConfig};
use logscope::models::{Level, LogEntry};

fn entry(ts_secs: u32, level: Level, source: &str, line: &str) -> LogEntry {
    LogEntry {
        timestamp: Utc.timestamp_opt(ts_secs as i64, 0).unwrap(),
        raw_lines: vec![line.to_string()],
        source: source.to_string(),
        original_line_number: 1,
        level,
    }
}

#[test]
#[ignore = "requires a live OpenAI-compatible endpoint"]
fn live_analysis_round_trip() {
    let base_url = std::env::var("LLM_BASE_URL")
        .unwrap_or_else(|_| "http://192.168.50.7:8731/v1".to_string());
    let api_key = std::env::var("LLM_API_KEY").unwrap_or_default();
    let model = std::env::var("LLM_MODEL")
        .unwrap_or_else(|_| "halogen-qwen3.8-flash-next".to_string());

    let entries = vec![
        entry(1_760_000_000, Level::Info, "syslog", "sshd[101]: Accepted publickey for dev from 203.0.113.9 port 52001"),
        entry(1_760_000_060, Level::Warning, "syslog", "kernel: CPU temp above threshold: 85C"),
        entry(1_760_000_120, Level::Error, "app", "database connection pool exhausted after 30s timeout"),
        entry(1_760_000_180, Level::Critical, "app", "unhandled exception in request handler; service restarting"),
        entry(1_760_000_240, Level::Info, "app", "service restarted successfully, listening on :8080"),
    ];

    let cfg = LlmConfig {
        base_url,
        api_key,
        model,
        max_prompt_chars: 0,
    };
    let result = llm::analyze(&entries, &cfg)
        .expect("live analysis call failed — is the endpoint up?");

    eprintln!("summary: {}", result.summary);
    eprintln!("key_events: {}", result.key_events.len());
    for ev in &result.key_events {
        eprintln!("  {} [{}] {} ({})", ev.timestamp, ev.level, ev.title, ev.source);
    }
    assert!(!result.summary.trim().is_empty(), "summary should not be empty");
    assert!(
        !result.key_events.is_empty(),
        "expected at least one key event"
    );
}
