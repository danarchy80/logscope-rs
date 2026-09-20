# LogScope Sanitization Feature

## Goal
Add log sanitization to redact sensitive information before export:
- **IP addresses** (IPv4 + IPv6) — automatic detection via regex
- **System names** — user-provided list
- **Account names** — user-provided list

## Design

### 1. Core sanitization module: `src/core/sanitize.rs`

```rust
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
        // Matches IPv4: 1-3 digits.1-3 digits.1-3 digits.1-3 digits
        // Word boundaries prevent matching inside longer numbers.
        Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap()
    })
}

fn ipv6_regex() -> &'static Regex {
    IPV6_RE.get_or_init(|| {
        // Simplified IPv6: matches common forms including ::, full, and mixed
        // Full form: 8 groups of 1-4 hex digits separated by colons
        // Compressed: :: can replace one or more groups of zeros
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

    // 1. IPs (regex-based)
    if config.redact_ips {
        result = ipv4_regex().replace_all(&result, IP_TOKEN).to_string();
        result = ipv6_regex().replace_all(&result, IP_TOKEN).to_string();
    }

    // 2. System names (case-insensitive exact match, word boundaries)
    for name in &config.system_names {
        if name.is_empty() {
            continue;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(name));
        if let Ok(re) = Regex::new(&pattern) {
            result = re.replace_all(&result, SYSTEM_TOKEN).to_string();
        }
    }

    // 3. Account names (case-insensitive exact match, word boundaries)
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
```

### 2. `src/core/mod.rs`
Add `pub mod sanitize;` (alphabetical order after `parser`).

### 3. GUI integration: `src/gui/app.rs`

#### 3a. Struct fields
Add to `LogScopeApp`:
```rust
sanitize: SanitizeConfig,
sanitize_text: String,  // comma-separated system names
sanitize_accounts: String,  // comma-separated account names
```

`Default::default()`:
```rust
sanitize: SanitizeConfig::default(),
sanitize_text: String::new(),
sanitize_accounts: String::new(),
```

#### 3b. Controls UI
Add a new horizontal row after the "Levels" row and before the Export button:

```rust
ui.horizontal(|ui| {
    ui.checkbox(&mut self.sanitize.redact_ips, "Redact IPs")
        .on_hover_text("Replace IPv4 and IPv6 addresses with [REDACTED_IP].");
    ui.label("Systems (comma):");
    ui.text_edit_singleline(&mut self.sanitize_text)
        .on_hover_text("Comma-separated system/host names to redact (case-insensitive).");
    ui.label("Accounts (comma):");
    ui.text_edit_singleline(&mut self.sanitize_accounts)
        .on_hover_text("Comma-separated account/user names to redact (case-insensitive).");
});
```

#### 3c. Parse sanitize fields before export
In `apply_filters`, after parsing `sources_filter` and `search`, add:

```rust
// Parse sanitize fields
self.sanitize.system_names = self.sanitize_text
    .split(',')
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect();
self.sanitize.account_names = self.sanitize_accounts
    .split(',')
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect();
```

Then, after `self.filtered = filter_entries_with_options(...)`, add:

```rust
// Apply sanitization if active
if self.sanitize.is_active() {
    crate::core::sanitize::sanitize_entries(&mut self.filtered, &self.sanitize);
}
```

#### 3d. Import
Add at top:
```rust
use crate::core::sanitize::SanitizeConfig;
```

### 4. Tests: `tests/test_sanitize.rs`

```rust
use logscope::core::sanitize::{sanitize_text, sanitize_entry, SanitizeConfig, IP_TOKEN, SYSTEM_TOKEN, ACCOUNT_TOKEN};
use logscope::models::{Level, LogEntry};

fn make_entry(raw: &str) -> LogEntry {
    LogEntry {
        timestamp: chrono::Utc::now(),
        raw_lines: vec![raw.to_string()],
        source: "test.log".to_string(),
        original_line_number: 0,
        level: Level::Info,
    }
}

#[test]
fn no_sanitization_when_inactive() {
    let config = SanitizeConfig::default();
    assert!(!config.is_active());
    let text = "192.168.1.1 user admin";
    assert_eq!(sanitize_text(text, &config), text);
}

#[test]
fn redact_ipv4() {
    let mut config = SanitizeConfig::new();
    config.redact_ips = true;
    let text = "Connection from 192.168.1.100 to 10.0.0.1";
    let result = sanitize_text(text, &config);
    assert!(result.contains(IP_TOKEN));
    assert!(!result.contains("192.168.1.100"));
    assert!(!result.contains("10.0.0.1"));
}

#[test]
fn redact_ipv6() {
    let mut config = SanitizeConfig::new();
    config.redact_ips = true;
    let text = "IPv6 address 2001:db8::1 and fe80::1";
    let result = sanitize_text(text, &config);
    assert!(result.contains(IP_TOKEN));
    assert!(!result.contains("2001:db8::1"));
}

#[test]
fn redact_system_names() {
    let mut config = SanitizeConfig::new();
    config.system_names = vec!["webserver01".to_string(), "db-server".to_string()];
    let text = "Error on webserver01 and DB-SERVER (case-insensitive)";
    let result = sanitize_text(text, &config);
    assert!(result.contains(SYSTEM_TOKEN));
    assert!(!result.contains("webserver01"));
    assert!(!result.contains("DB-SERVER"));
}

#[test]
fn redact_account_names() {
    let mut config = SanitizeConfig::new();
    config.account_names = vec!["admin".to_string(), "jdoe".to_string()];
    let text = "User admin logged in, then jdoe logged out";
    let result = sanitize_text(text, &config);
    assert!(result.contains(ACCOUNT_TOKEN));
    assert!(!result.contains("admin"));
    assert!(!result.contains("jdoe"));
}

#[test]
fn combined_sanitization() {
    let mut config = SanitizeConfig::new();
    config.redact_ips = true;
    config.system_names = vec!["prod-server".to_string()];
    config.account_names = vec!["root".to_string()];
    let text = "192.168.1.1 root@prod-server login failed";
    let result = sanitize_text(text, &config);
    assert!(result.contains(IP_TOKEN));
    assert!(result.contains(SYSTEM_TOKEN));
    assert!(result.contains(ACCOUNT_TOKEN));
    assert!(!result.contains("192.168.1.1"));
    assert!(!result.contains("root"));
    assert!(!result.contains("prod-server"));
}

#[test]
fn sanitize_entry_modifies_raw_lines() {
    let mut config = SanitizeConfig::new();
    config.redact_ips = true;
    let mut entry = make_entry("Connection from 10.0.0.5");
    sanitize_entry(&mut entry, &config);
    assert!(entry.raw_lines[0].contains(IP_TOKEN));
    assert!(!entry.raw_lines[0].contains("10.0.0.5"));
}

#[test]
fn word_boundaries_prevent_partial_matches() {
    let mut config = SanitizeConfig::new();
    config.account_names = vec!["admin".to_string()];
    let text = "administrator and admin logged in";
    let result = sanitize_text(text, &config);
    // "administrator" should NOT be redacted (word boundary)
    assert!(result.contains("administrator"));
    // "admin" should be redacted
    assert!(result.contains(ACCOUNT_TOKEN));
    assert!(!result.contains(" admin "));
}
```

## Acceptance criteria
- `cargo test` compiles and ALL tests pass (82 existing + 8 new = 90 total).
- `cargo build` succeeds for both binaries.
- GUI shows the new sanitization row with checkboxes and text fields.
- Export applies sanitization when any redaction is enabled.
- No sanitization occurs when all fields are empty/unchecked.