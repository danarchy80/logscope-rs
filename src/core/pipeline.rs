//! End-to-end pipeline — ported from Python `logscope/core/pipeline.py`.
//!
//! ingest → filter → export, returning counts.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::models::Level;
use crate::core::export::export_entries;
use crate::core::filter::{filter_entries_with_options, FilterOptions};
use crate::core::ingest::{ingest, IngestError};

/// Counts reported by [`run_pipeline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineResult {
    pub total_entries: usize,
    pub filtered_entries: usize,
    pub sources: usize,
}

#[derive(Debug, Clone)]
pub struct PipelineOptions {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub levels: Option<Vec<Level>>,
    pub sources: Option<Vec<String>>,
    pub search: Option<String>,
}

pub fn run_pipeline_with_options(
    inputs: &[PathBuf],
    opts: &PipelineOptions,
    output_path: &Path,
) -> Result<PipelineResult, IngestError> {
    let mut all_sources = Vec::new();
    for input in inputs {
        let sources = ingest(input)?;
        all_sources.extend(sources);
    }
    
    let total: usize = all_sources.iter().map(|s| s.entries.len()).sum();
    let filter_opts = FilterOptions {
        start: opts.start,
        end: opts.end,
        levels: opts.levels.clone(),
        sources: opts.sources.clone(),
        search: opts.search.clone(),
    };
    
    let filtered = filter_entries_with_options(&all_sources, &filter_opts);
    export_entries(&filtered, output_path)?;
    Ok(PipelineResult {
        total_entries: total,
        filtered_entries: filtered.len(),
        sources: all_sources.len(),
    })
}

/// Ingest `input_path`, keep entries within `[start, end]`, write them to
/// `output_path`, and return entry/source counts.
pub fn run_pipeline(
    input_path: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    output_path: &Path,
) -> Result<PipelineResult, IngestError> {
    let opts = PipelineOptions {
        start,
        end,
        levels: None,
        sources: None,
        search: None,
    };
    run_pipeline_with_options(&[input_path.to_path_buf()], &opts, output_path)
}

/// Ingest `inputs`, apply time + source filters (levels/search ignored — the
/// heatmap shows severity distribution), and return the built `Heatmap` plus
/// the number of entries it represents.
pub fn run_heatmap_data(
    inputs: &[PathBuf],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    sources: Option<Vec<String>>,
    num_buckets: usize,
) -> Result<(crate::core::heatmap::Heatmap, usize), IngestError> {
    let mut all_sources = Vec::new();
    for input in inputs {
        let src = ingest(input)?;
        all_sources.extend(src);
    }

    let filter_opts = FilterOptions {
        start,
        end,
        levels: None,
        sources,
        search: None,
    };
    let filtered = filter_entries_with_options(&all_sources, &filter_opts);
    let count = filtered.len();

    Ok((crate::core::heatmap::build_heatmap(&filtered, num_buckets), count))
}

/// Ingest `inputs`, apply time + source filters (levels/search ignored — the
/// heatmap shows severity distribution), build an SVG heatmap with
/// `num_buckets` columns, and write it to `output_path`. Returns the number of
/// entries represented.
pub fn run_heatmap(
    inputs: &[PathBuf],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    sources: Option<Vec<String>>,
    num_buckets: usize,
    output_path: &Path,
) -> Result<usize, IngestError> {
    let (heatmap, count) = run_heatmap_data(inputs, start, end, sources, num_buckets)?;
    let svg = crate::core::heatmap::render_heatmap_svg(&heatmap);
    std::fs::write(output_path, svg)?;

    Ok(count)
}
