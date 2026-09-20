//! Core submodules. Phase 1 provides `parser`; later phases add ingest,
//! filter, normalize, export, and pipeline here.

pub mod evtx;
pub mod export;
pub mod filter;
pub mod heatmap;
pub mod ingest;
pub mod level;
pub mod normalize;
pub mod parser;
pub mod pipeline;
pub mod workspace;
