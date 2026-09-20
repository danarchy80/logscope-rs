//! Log sanitization — redact sensitive information from log entries.

use regex::Regex;
use std::sync::OnceLock;

/// Replacement tokens for each redaction category.
pub const IP_TOKEN: &str = "[REDACTED_IP]";
pub const SYSTEM_TOKEN: &str = "[REDACTED_SYSTEM]";
pub const ACCOUNT_TOKEN: &str = "[REDACTED_ACCOUNT]";

/// Compiled regexes (lazy-initialized, thread-safe).
static IPV4_RE: OnceLock<Regex> = OnceLock::new();
static IPV6_RE: OnceLock<Regex> = OnceLock::new();

fn ipv4_regex() -> &'static Regex {
    IPV4_RE.get_or_init(|| {
        Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap()
    })
}

fn ipv6_regex() -> &'static Regex {
    IPV6_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}\b|\b(?:[0-9a-f]{1,4}:){1,7}:\b|\b(?:[0-9a-f]{1,4}:){1,6}:[0-9a-f]{1,4}\b|\b(?:[0-9a-f]{1,4}:){1,5}(?::[0-9a-f]{1,4}){1,2}\b|\b(?:[0-9a-f]{1,4}:){1,4}(?::[0-9a-f]{1,4}){1,3}\b|\b(?:[0-9a-f]{1,4}:){1,3}(?::[0-9a-f]{1,4}){1,4}\b|\b(?:[0-9a-f]{1,4}:){1,2}(?::[0-9a-f]{1,4}){1,5}\b|\b[0-9a-f]{1,4}:(?::[0-9a-f]{1,4}){1,6}\b|\b:(?::[0-9a-f]{1,4}){1,7}\b|\b::(?:[0-9a-f]{1,4}:){0,5}[0-9a-f]{1,4}\b|\b[0-9a-f]{1,4}::(?:[0-9a-f]{1,4}:){0,4}[0-9a-f]{1,4}\b").unwrap()
    })
}

/// Sanitization configuration.
#[derive(Debug, Clone, Default)]
pub struct SanitizeConfig {
    /// Redact IP addresses (IPv4 + IPv6).
    pub redact_ips: bool,
    /// Redact these system/host names (case-insensitive exact match).
    pub system_names: Vec<String>,
    /// Redact these account/user names (case-insensitive exact match).
    pub account_names: Vec<String>,
}

impl SanitizeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// True if any redaction is enabled.
    pub fn is_active(&self) -> bool {
        self.redact_ips || !self.system_names.is_empty() || !self.account_names.is_empty()
    }
}

/// Sanitize a single string by applying all enabled redactions.
pub fn sanitize_text(text: &str, config: &SanitizeConfig) -> String {
    if !config.is_active() {
        return text.to_string();
    }

    let mut result = text.to_string();

    if config.redact_ips {
        result = ipv4_regex().replace_all(&result, IP_TOKEN).to_string();
        result = ipv6_regex().replace_all(&result, IP_TOKEN).to_string();
    }

    for name in &config.system_names {
        if name.is_empty() {
            continue;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(name));
        if let Ok(re) = Regex::new(&pattern) {
            result = re.replace_all(&result, SYSTEM_TOKEN).to_string();
        }
    }

    for name in &config.account_names {
        if name.is_empty() {
            continue;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(name));
        if let Ok(re) = Regex::new(&pattern) {
            result = re.replace_all(&result, ACCOUNT_TOKEN).to_string();
        }
    }

    result
}

/// Sanitize all raw_lines in a LogEntry.
pub fn sanitize_entry(entry: &mut crate::models::LogEntry, config: &SanitizeConfig) {
    if !config.is_active() {
        return;
    }
    for line in &mut entry.raw_lines {
        *line = sanitize_text(line, config);
    }
}

/// Sanitize all entries in a slice (in-place).
pub fn sanitize_entries(entries: &mut [crate::models::LogEntry], config: &SanitizeConfig) {
    if !config.is_active() {
        return;
    }
    for entry in entries {
        sanitize_entry(entry, config);
    }
}
