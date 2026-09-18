//! `logscope-cli` — command-line front-end for the LogScope pipeline.
//!
//! Usage: logscope-cli -i <input> [-s <start>] [-e <end>] [-o <output>]

use std::path::PathBuf;
use std::process::exit;

use chrono::{DateTime, Utc};
use clap::Parser;

use logscope::core::pipeline::{run_pipeline_with_options, run_heatmap, PipelineOptions};
use logscope::models::Level;
use logscope::datetime::parse_datetime;

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

/// LogScope: ingest logs, filter by time range and search, export a unified log.
#[derive(Parser, Debug)]
#[command(name = "logscope-cli", about = "Filter and unify log files by time range and search")]
struct Cli {
    /// Input path: file, directory, .zip, or .tar/.tar.gz/.tgz/.tar.bz2/.tbz2
    #[arg(short = 'i', long = "input", required = true)]
    input: Vec<PathBuf>,

    /// Inclusive start of the time range (RFC 3339 or "YYYY-MM-DD HH:MM:SS")
    #[arg(short = 's', long = "start")]
    start: Option<String>,

    /// Inclusive end of the time range (RFC 3339 or "YYYY-MM-DD HH:MM:SS")
    #[arg(short = 'e', long = "end")]
    end: Option<String>,

    /// Output file path
    #[arg(short = 'o', long = "output", default_value = "unified.log")]
    output: PathBuf,

    /// Filter by severity levels (comma-separated, e.g., "error,warn")
    #[arg(long = "level")]
    level: Vec<String>,

    /// Filter by source substring
    #[arg(long = "source")]
    source: Vec<String>,

    /// Filter by search substring
    #[arg(long = "search")]
    search: Option<String>,

    /// Output heatmap SVG path
    #[arg(long = "heatmap")]
    heatmap: Option<PathBuf>,

    /// Number of heatmap buckets
    #[arg(long = "buckets", default_value_t = 60)]
    buckets: usize,
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
    
    let levels = if cli.level.is_empty() {
        None
    } else {
        let mut parsed_levels = Vec::new();
        for t in cli.level.iter().flat_map(|s| s.split(',')) {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            if let Some(lvl) = Level::parse(t) {
                parsed_levels.push(lvl);
            } else {
                eprintln!("error: --level: unknown level {t}");
                exit(2);
            }
        }
        if parsed_levels.is_empty() { None } else { Some(parsed_levels) }
    };
    
    let sources = if cli.source.is_empty() {
        None
    } else {
        Some(cli.source.clone())
    };

    let opts = PipelineOptions {
        start,
        end,
        levels,
        sources: sources.clone(),
        search: cli.search,
    };

    match run_pipeline_with_options(&cli.input, &opts, &cli.output) {
        Ok(res) => {
            println!(
                "Exported {} of {} entries from {} source(s) to {}",
                res.filtered_entries,
                res.total_entries,
                res.sources,
                cli.output.display()
            );
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit(1);
        }
    }

    if let Some(path) = cli.heatmap {
        match run_heatmap(&cli.input, start, end, sources, cli.buckets, &path) {
            Ok(_) => println!("Heatmap written to {}", path.display()),
            Err(e) => {
                eprintln!("error: {e}");
                exit(1);
            }
        }
    }
    
    exit(0);
}
