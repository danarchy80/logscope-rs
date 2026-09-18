use crate::models::Level;
use regex::Regex;
use std::sync::LazyLock;

static RE_CRITICAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(CRITICAL|FATAL|CRIT|ALERT|EMERG|EMERGENCY)\b").unwrap());
static RE_ERROR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(ERROR|ERR)\b").unwrap());
static RE_WARNING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(WARN|WARNING)\b").unwrap());
static RE_INFO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(INFO|NOTICE)\b").unwrap());
static RE_DEBUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(DEBUG|TRACE|VERBOSE)\b").unwrap());
static RE_SYSLOG_PRI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<(\d+)>").unwrap());

/// Detect a severity level from an entry's full raw lines.
///
/// Priority order (first hit wins):
/// 1. Syslog `<PRI>`: if the FIRST line starts with `<digits>`, severity = PRI % 8.
///    Map 0..=2 -> Critical, 3 -> Error, 4 -> Warning, 5|6 -> Info, 7 -> Debug.
/// 2. Keyword regex, case-insensitive, checked in this order (most severe first):
///      Critical: \b(CRITICAL|FATAL|CRIT|ALERT|EMERG|EMERGENCY)\b
///      Error:    \b(ERROR|ERR)\b
///      Warning:  \b(WARN|WARNING)\b
///      Info:     \b(INFO|NOTICE)\b
///      Debug:    \b(DEBUG|TRACE|VERBOSE)\b
///    Scan across ALL lines joined with '\n'.
/// 3. Default Level::Unknown.
pub fn detect_level(raw_lines: &[String]) -> Level {
    if raw_lines.is_empty() {
        return Level::Unknown;
    }

    if let Some(caps) = RE_SYSLOG_PRI.captures(&raw_lines[0]) {
        if let Ok(pri) = caps[1].parse::<u32>() {
            let sev = pri % 8;
            return match sev {
                0..=2 => Level::Critical,
                3 => Level::Error,
                4 => Level::Warning,
                5..=6 => Level::Info,
                7 => Level::Debug,
                _ => Level::Unknown, // Should be unreachable for % 8
            };
        }
    }

    let joined = raw_lines.join("\n");
    if RE_CRITICAL.is_match(&joined) {
        return Level::Critical;
    }
    if RE_ERROR.is_match(&joined) {
        return Level::Error;
    }
    if RE_WARNING.is_match(&joined) {
        return Level::Warning;
    }
    if RE_INFO.is_match(&joined) {
        return Level::Info;
    }
    if RE_DEBUG.is_match(&joined) {
        return Level::Debug;
    }

    Level::Unknown
}
