//! Chat-completions support for the built-in AI assistant window.
//!
//! Pure prompt/payload logic plus a blocking HTTP call (ureq 3), mirroring the
//! HTTP pattern in [`crate::core::llm::analyze`]. No GUI dependencies.

use std::time::Duration;

use serde_json::json;

use crate::core::llm::LlmConfig;

/// HTTP timeout for the chat-completions call.
const TIMEOUT_SECS: u64 = 180;

/// Max characters of response body included in error messages.
const ERROR_BODY_PREVIEW: usize = 200;

/// One chat turn. `role` is "user" or "assistant".
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Session snapshot handed to the model so it understands the tool + data.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub sources: usize,
    pub entries: usize,
    pub filtered: usize,
    /// Already formatted strings.
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    /// e.g. "levels=ERROR,WARNING", "search=timeout".
    pub active_filters: Vec<String>,
    pub redaction_active: bool,
    pub redact_ips: bool,
    pub redact_system_names: bool,
    pub redact_account_names: bool,
}

/// Build the system prompt: purpose paragraph + current session lines + guidance.
pub fn tool_context(tc: &ToolContext) -> String {
    let mut out = String::new();

    // 1. Purpose paragraph.
    out.push_str(
        "You are the built-in assistant of LogScope, a desktop log-analysis \
         tool for incident triage. LogScope ingests multi-source logs (plain text, \
         Windows .evtx, zip/tar archives), normalizes timestamps into one unified \
         timeline, and offers: level/source/search/time-window filtering, a time \
         heatmap, key-event AI analysis, a redaction pipeline (IP addresses, system \
         names, account names) applied before any data leaves the machine, and \
         unified export.",
    );
    out.push('\n');

    // 2. Current session lines.
    let time_window = match (&tc.time_start, &tc.time_end) {
        (Some(s), Some(e)) => format!("{s} .. {e}"),
        (Some(s), None) => format!("{s} .."),
        (None, Some(e)) => format!(".. {e}"),
        (None, None) => "unbounded".to_string(),
    };
    let filters = if tc.active_filters.is_empty() {
        "none".to_string()
    } else {
        tc.active_filters.join(", ")
    };
    let redaction = if tc.redaction_active {
        let mut parts: Vec<&str> = Vec::new();
        if tc.redact_ips {
            parts.push("ips");
        }
        if tc.redact_system_names {
            parts.push("system names");
        }
        if tc.redact_account_names {
            parts.push("account names");
        }
        if parts.is_empty() {
            "redaction: ON".to_string()
        } else {
            format!("redaction: ON ({})", parts.join(", "))
        }
    } else {
        "redaction: OFF".to_string()
    };

    out.push_str("Current session:\n");
    out.push_str(&format!("- sources: {}\n", tc.sources));
    out.push_str(&format!("- total entries: {}\n", tc.entries));
    out.push_str(&format!("- filtered entries: {}\n", tc.filtered));
    out.push_str(&format!("- time window: {time_window}\n"));
    out.push_str(&format!("- active filters: {filters}\n"));
    out.push_str(&format!("- {redaction}\n"));

    // 3. Guidance line.
    out.push_str(
        "When asked about the logs, base answers only on the log excerpt provided. \
         Prefer concise markdown. If data was redacted, refer to placeholders like \
         [REDACTED_IP].",
    );

    out
}

/// Build the chat/completions JSON payload (testable, no network).
pub fn build_chat_payload(
    system: &str,
    history: &[ChatMessage],
    model: &str,
) -> serde_json::Value {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for m in history {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }
    json!({
        "model": model,
        "temperature": 0.3,
        "messages": messages,
    })
}

/// Send chat to `{base_url}/chat/completions`; returns assistant content (trimmed).
pub fn chat(
    system: &str,
    history: &[ChatMessage],
    cfg: &LlmConfig,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let payload = build_chat_payload(system, history, &cfg.model);

    let mut req = ureq::post(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
        .http_status_as_error(false)
        .build();
    if !cfg.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", cfg.api_key));
    }

    let mut res = req.send_json(payload).map_err(|e| format!("{e}"))?;
    let status = res.status();
    let text = res
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed to read response body: {e}"))?;

    if !status.is_success() {
        let preview: String = text.chars().take(ERROR_BODY_PREVIEW).collect();
        return Err(format!("HTTP {status}: {preview}"));
    }

    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid JSON response: {e}"))?;
    let content = value["choices"]
        .get(0)
        .and_then(|c| c["message"].get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "response missing choices[0].message.content".to_string())?;

    Ok(content.trim().to_string())
}
