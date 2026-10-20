//! Live chat round-trip against an OpenAI-compatible endpoint. Opt-in:
//! `cargo test --test test_chat_live -- --ignored`. Env: LLM_BASE_URL / LLM_API_KEY / LLM_MODEL.

use logscope::core::chat::{self, ChatMessage, ToolContext};
use logscope::core::llm::LlmConfig;

#[test]
#[ignore = "requires a live OpenAI-compatible endpoint"]
fn live_chat_with_tool_context() {
    let cfg = LlmConfig {
        base_url: std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "http://192.168.50.7:8731/v1".to_string()),
        api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
        model: std::env::var("LLM_MODEL")
            .unwrap_or_else(|_| "halogen-qwen3.8-flash-next".to_string()),
        max_prompt_chars: 0,
    };
    let tc = ToolContext {
        sources: 2,
        entries: 150,
        filtered: 42,
        time_start: None,
        time_end: None,
        active_filters: vec!["levels=ERROR,WARNING".to_string()],
        redaction_active: true,
        redact_ips: true,
        redact_system_names: false,
        redact_account_names: false,
    };
    let system = format!(
        "{}\n\nLog excerpt (filtered):\n2025-10-09 08:55:20.000 [ERROR] (app) database connection pool exhausted\n2025-10-09 08:56:20.000 [CRITICAL] (app) unhandled exception; restarting",
        chat::tool_context(&tc)
    );
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: "What is the most serious problem in these logs?".to_string(),
    }];
    let reply = chat::chat(&system, &history, &cfg)
        .expect("live chat failed — is the endpoint up?");
    eprintln!("reply: {reply}");
    assert!(!reply.trim().is_empty());
    let lower = reply.to_lowercase();
    assert!(
        lower.contains("database") || lower.contains("pool") || lower.contains("exception"),
        "reply should reference the log content: {reply}"
    );
}
