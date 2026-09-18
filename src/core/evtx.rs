use evtx::EvtxParser;
use crate::core::ingest::IngestError;
use crate::models::{Level, LogEntry};
use regex::Regex;
use std::sync::LazyLock;
use chrono::DateTime;
use chrono::Utc;

static RE_EVTX_LEVEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<Level>\s*(\d+)\s*</Level>").unwrap());

/// Map a Windows Event Log numeric level to our Level.
/// 1=Critical, 2=Error, 3=Warning, 4=Information->Info, 5=Verbose->Debug, else Unknown.
pub fn map_evtx_level(n: u32) -> Level {
    match n {
        1 => Level::Critical,
        2 => Level::Error,
        3 => Level::Warning,
        4 => Level::Info,
        5 => Level::Debug,
        _ => Level::Unknown,
    }
}

/// Parse EVTX bytes (XML rendering) into LogEntry list.
/// Top-level open/header failure -> Err(IngestError). Individual record parse
/// failures are SKIPPED (best-effort, mirrors archive-member handling).
/// Each record becomes ONE LogEntry:
///   timestamp = record.timestamp (jiff Timestamp) -> chrono DateTime<Utc>
///   level     = map_evtx_level(level parsed from "<Level>N</Level>" in the XML)
///   raw_lines = the record's XML text split on '\n' (Vec<String>)
///   source    = name, original_line_number = 0
pub fn parse_evtx(name: &str, bytes: &[u8]) -> Result<Vec<LogEntry>, IngestError> {
    let mut parser = EvtxParser::from_buffer(bytes.to_vec())
        .map_err(|e| IngestError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))?;

    let mut entries = Vec::new();

    for record_result in parser.records() {
        if let Ok(record) = record_result {
            let secs = record.timestamp.as_second();
            let nanos = record.timestamp.subsec_nanosecond() as u32;
            let timestamp = DateTime::<Utc>::from_timestamp(secs, nanos).unwrap_or_default();
            
            let xml = record.data.clone();
            
            let level = if let Some(caps) = RE_EVTX_LEVEL.captures(&xml) {
                if let Ok(n) = caps[1].parse::<u32>() {
                    map_evtx_level(n)
                } else {
                    Level::Unknown
                }
            } else {
                Level::Unknown
            };

            let raw_lines = xml.split('\n').map(|s| s.to_string()).collect::<Vec<String>>();

            entries.push(LogEntry {
                timestamp,
                raw_lines,
                source: name.to_string(),
                original_line_number: 0,
                level,
            });
        }
    }

    Ok(entries)
}
