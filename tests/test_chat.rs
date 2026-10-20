//! Tests for `core::chat` — pure logic only, no network.

use logscope::core::chat::{build_chat_payload, tool_context, ChatMessage, ToolContext};

fn full_context() -> ToolContext {
    ToolContext {
        sources: 3,
        entries: 120,
        filtered: 45,
        time_start: Some("2026-10-01 00:00:00".to_string()),
        time_end: Some("2026-10-20 12:00:00".to_string()),
        active_filters: vec!["levels=ERROR,WARNING".to_string(), "search=timeout".to_string()],
        redaction_active: true,
        redact_ips: true,
        redact_system_names: true,
        redact_account_names: true,
    }
}

// Group 1: purpose paragraph mentions + numeric session values.
#[test]
fn tool_context_mentions_purpose_and_session_numbers() {
    let out = tool_context(&full_context());
    for needle in ["LogScope", "redaction", "heatmap", "unified"] {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }
    assert!(out.contains("sources: 3"), "missing sources digit");
    assert!(out.contains("total entries: 120"), "missing entries digit");
    assert!(out.contains("filtered entries: 45"), "missing filtered digit");
    assert!(out.contains("Current session:"));
    assert!(out.contains("[REDACTED_IP]"), "missing guidance line");
}

// Group 2: redaction status line.
#[test]
fn tool_context_redaction_on_lists_flags() {
    let out = tool_context(&full_context());
    assert!(out.contains("redaction: ON"), "missing ON:\n{out}");
    assert!(out.contains("ips"));
    assert!(out.contains("system names"));
    assert!(out.contains("account names"));
}

#[test]
fn tool_context_redaction_off() {
    let mut tc = full_context();
    tc.redaction_active = false;
    let out = tool_context(&tc);
    assert!(out.contains("redaction: OFF"), "missing OFF:\n{out}");
    assert!(!out.contains("redaction: ON"));
}

// Group 3: empty filters => "none"; no time bounds => "unbounded".
#[test]
fn tool_context_empty_filters_and_unbounded_time() {
    let tc = ToolContext::default();
    let out = tool_context(&tc);
    assert!(out.contains("active filters: none"), "missing none:\n{out}");
    assert!(out.contains("unbounded"), "missing unbounded:\n{out}");
}

// Group 4: payload shape.
#[test]
fn build_chat_payload_shape() {
    let history = vec![
        ChatMessage { role: "user".into(), content: "hi".into() },
        ChatMessage { role: "assistant".into(), content: "hello".into() },
        ChatMessage { role: "user".into(), content: "again".into() },
    ];
    let payload = build_chat_payload("SYS", &history, "test-model");

    assert_eq!(payload["model"], "test-model");
    assert_eq!(payload["temperature"], 0.3);

    let msgs = payload["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "SYS");
    assert_eq!(msgs[1]["role"], "user");
    assert_eq!(msgs[1]["content"], "hi");
    assert_eq!(msgs[2]["role"], "assistant");
    assert_eq!(msgs[2]["content"], "hello");
    assert_eq!(msgs[3]["role"], "user");
    assert_eq!(msgs[3]["content"], "again");
}

#[test]
fn build_chat_payload_empty_history() {
    let payload = build_chat_payload("SYS", &[], "m");
    let msgs = payload["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "system");
}
