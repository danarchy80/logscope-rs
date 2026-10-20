//! LLM log analysis via an OpenAI-compatible chat-completions endpoint.
//!
//! Pure prompt-building/parsing logic plus a blocking HTTP call (ureq 3).
//! No GUI dependencies — wired into the app by the GUI layer.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::models::LogEntry;

/// Default prompt budget when `LlmConfig::max_prompt_chars` is 0.
pub const DEFAULT_MAX_PROMPT_CHARS: usize = 24_000;

/// Allowed slack above `max_prompt_chars` for instruction overhead.
pub const INSTRUCTION_OVERHEAD: usize = 1_000;

/// HTTP timeout for the chat-completions call.
const TIMEOUT_SECS: u64 = 180;

/// Max characters of response body included in error messages.
const ERROR_BODY_PREVIEW: usize = 200;

/// Connection settings for an OpenAI-compatible server.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LlmConfig {
    /// e.g. `http://192.168.50.7:8731/v1` (trailing slash tolerated).
    pub base_url: String,
    /// Empty => no `Authorization` header (local servers).
    pub api_key: String,
    /// e.g. `halogen-qwen3.8-flash-next`.
    pub model: String,
    /// 0 => [`DEFAULT_MAX_PROMPT_CHARS`].
    pub max_prompt_chars: usize,
}

/// One event extracted by the model for the timeline view.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyEvent {
    /// Verbatim from logs, free-form.
    pub timestamp: String,
    /// "CRITICAL" | "ERROR" | "WARNING" | "INFO" | "OTHER".
    pub level: String,
    pub title: String,
    pub detail: String,
    pub source: String,
}

/// Parsed model output.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisResult {
    pub summary: String,
    pub key_events: Vec<KeyEvent>,
    pub recommendations: Vec<String>,
}

/// System instruction: demand a strict JSON object response.
fn instruction() -> String {
    "You are a log-analysis assistant. Respond ONLY with a JSON object of the form \
     {\"summary\": string, \"key_events\": [{\"timestamp\": string, \"level\": string, \
     \"title\": string, \"detail\": string, \"source\": string}], \"recommendations\": [string]}. \
     Each key_event level must be one of CRITICAL, ERROR, WARNING, INFO, OTHER. \
     Do not include any text outside the JSON object."
        .to_string()
}

/// Render one entry as `{timestamp} [{LEVEL}] ({source}) {first_raw_line}`.
fn render_entry(e: &LogEntry) -> String {
    let ts = e.timestamp.format("%Y-%m-%d %H:%M:%S%.3f");
    let msg = e.raw_lines.first().map(String::as_str).unwrap_or("");
    format!("{} [{}] ({}) {}", ts, e.level.as_str(), e.source, msg)
}

/// Build the full prompt (instruction block + log body), truncated to budget.
pub fn build_prompt(entries: &[LogEntry], max_chars: usize) -> String {
    let budget = if max_chars == 0 { DEFAULT_MAX_PROMPT_CHARS } else { max_chars };
    let body = build_body(entries, budget);
    format!("{}\n\n{}", instruction(), body)
}

/// Render entries one per line; if over budget keep the first ~60% and last
/// ~40% (by entry count fitting the char budget) joined by a truncation marker.
fn build_body(entries: &[LogEntry], budget: usize) -> String {
    let lines: Vec<String> = entries.iter().map(render_entry).collect();
    let total_chars: usize = lines.iter().map(|l| l.len() + 1).sum();
    if total_chars <= budget {
        return lines.join("\n");
    }

    let head_budget = budget * 60 / 100;
    let tail_budget = budget * 40 / 100;

    let mut head: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for line in &lines {
        if !head.is_empty() && used + line.len() + 1 > head_budget {
            break;
        }
        if line.len() > head_budget {
            break;
        }
        used += line.len() + 1;
        head.push(line.as_str());
    }
    let head_count = head.len();

    let mut tail: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for line in lines.iter().rev() {
        if head_count + tail.len() >= lines.len() {
            break;
        }
        if !tail.is_empty() && used + line.len() + 1 > tail_budget {
            break;
        }
        if line.len() > tail_budget {
            break;
        }
        used += line.len() + 1;
        tail.push(line.as_str());
    }
    tail.reverse();
    let tail_count = tail.len();

    let omitted = lines.len().saturating_sub(head_count + tail_count);
    let mut out = String::with_capacity(budget);
    for line in head {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!(
        "...[TRUNCATED {} of {} entries]...",
        omitted,
        lines.len()
    ));
    for line in tail {
        out.push('\n');
        out.push_str(line);
    }
    out
}

/// Parse model output into an [`AnalysisResult`].
///
/// Strips markdown code fences, takes the outermost `{ ... }` span, and
/// deserializes with tolerant defaults for missing fields.
pub fn parse_analysis(text: &str) -> Result<AnalysisResult, String> {
    let stripped = strip_code_fences(text);
    let start = stripped
        .find('{')
        .ok_or_else(|| "no JSON object found in model output".to_string())?;
    let end = stripped
        .rfind('}')
        .ok_or_else(|| "no JSON object found in model output".to_string())?;
    if end < start {
        return Err("no JSON object found in model output".to_string());
    }
    let json = &stripped[start..=end];
    serde_json::from_str::<AnalysisResult>(json)
        .map_err(|e| format!("invalid analysis JSON: {e}"))
}

/// Remove ```json ... ``` / ``` ... ``` fence markers.
fn strip_code_fences(text: &str) -> String {
    let mut out = text.replace("```json", "");
    out = out.replace("```JSON", "");
    out.replace("```", "")
}

/// Send entries to `{base_url}/chat/completions` and parse the analysis.
pub fn analyze(entries: &[LogEntry], cfg: &LlmConfig) -> Result<AnalysisResult, String> {
    let url = format!(
        "{}/chat/completions",
        cfg.base_url.trim_end_matches('/')
    );
    let budget = if cfg.max_prompt_chars == 0 {
        DEFAULT_MAX_PROMPT_CHARS
    } else {
        cfg.max_prompt_chars
    };
    let body = build_body(entries, budget);

    let payload = json!({
        "model": cfg.model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": instruction() },
            { "role": "user", "content": body },
        ],
    });

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

    parse_analysis(content)
}
