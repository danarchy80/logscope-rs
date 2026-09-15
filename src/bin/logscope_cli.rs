//! `logscope-cli` — command-line front-end for the LogScope pipeline.
//!
//! Usage: logscope-cli -i <input> [-s <start>] [-e <end>] [-o <output>]

use std::path::PathBuf;
use std::process::exit;

use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;

use logscope::core::pipeline::run_pipeline;

/// Far-past default lower bound (inclusive): 1970-01-01T00:00:00Z.
fn default_start() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

/// Far-future default upper bound (inclusive): 9999-12-31T23:59:59Z.
fn default_end() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("9999-12-31T23:59:59Z")
        .unwrap()
        .with_timezone(&Utc)
}

/// Parse a user-supplied datetime into UTC.
///
/// Tries, in order:
/// 1. RFC 3339 (e.g. `2026-08-26T10:00:00+00:00`)
/// 2. `%Y-%m-%d %H:%M:%S` (assumed UTC)
/// 3. `%Y-%m-%dT%H:%M:%S` (assumed UTC)
pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(naive.and_utc());
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(naive.and_utc());
    }
    Err(format!(
        "invalid datetime {s:?}: expected RFC 3339, \"YYYY-MM-DD HH:MM:SS\", or \"YYYY-MM-DDTHH:MM:SS\""
    ))
}

/// LogScope: ingest logs, filter by time range, export a unified log.
#[derive(Parser, Debug)]
#[command(name = "logscope-cli", about = "Filter and unify log files by time range")]
struct Cli {
    /// Input path: file, directory, .zip, or .tar/.tar.gz/.tgz/.tar.bz2/.tbz2
    #[arg(short = 'i', long = "input")]
    input: PathBuf,

    /// Inclusive start of the time range (RFC 3339 or "YYYY-MM-DD HH:MM:SS")
    #[arg(short = 's', long = "start")]
    start: Option<String>,

    /// Inclusive end of the time range (RFC 3339 or "YYYY-MM-DD HH:MM:SS")
    #[arg(short = 'e', long = "end")]
    end: Option<String>,

    /// Output file path
    #[arg(short = 'o', long = "output", default_value = "unified.log")]
    output: PathBuf,
}

fn main() {
    let cli = Cli::parse();

    let start = match cli.start.as_deref().map(parse_datetime) {
        Some(Ok(dt)) => dt,
        Some(Err(msg)) => {
            eprintln!("error: --start: {msg}");
            exit(2);
        }
        None => default_start(),
    };
    let end = match cli.end.as_deref().map(parse_datetime) {
        Some(Ok(dt)) => dt,
        Some(Err(msg)) => {
            eprintln!("error: --end: {msg}");
            exit(2);
        }
        None => default_end(),
    };

    if end < start {
        eprintln!("error: --end ({end}) is before --start ({start})");
        exit(2);
    }

    match run_pipeline(&cli.input, start, end, &cli.output) {
        Ok(res) => {
            println!(
                "Exported {} of {} entries from {} source(s) to {}",
                res.filtered_entries,
                res.total_entries,
                res.sources,
                cli.output.display()
            );
            exit(0);
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit(1);
        }
    }
}
