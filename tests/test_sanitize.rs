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
    assert!(result.contains("administrator"));
    assert!(result.contains(ACCOUNT_TOKEN));
    assert!(!result.contains(" admin "));
}
