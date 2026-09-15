//! End-to-end pipeline — ported from Python `logscope/core/pipeline.py`.
//!
//! ingest → filter → export, returning counts.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::core::export::export_entries;
use crate::core::filter::filter_entries;
use crate::core::ingest::{ingest, IngestError};

/// Counts reported by [`run_pipeline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineResult {
    pub total_entries: usize,
    pub filtered_entries: usize,
    pub sources: usize,
}

/// Ingest `input_path`, keep entries within `[start, end]`, write them to
/// `output_path`, and return entry/source counts.
pub fn run_pipeline(
    input_path: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    output_path: &Path,
) -> Result<PipelineResult, IngestError> {
    let sources = ingest(input_path)?;
    let total: usize = sources.iter().map(|s| s.entries.len()).sum();
    let filtered = filter_entries(&sources, start, end);
    export_entries(&filtered, output_path)?;
    Ok(PipelineResult {
        total_entries: total,
        filtered_entries: filtered.len(),
        sources: sources.len(),
    })
}
