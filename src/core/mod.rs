//! Core submodules. Phase 1 provides `parser`; later phases add ingest,
//! filter, normalize, export, and pipeline here.

pub mod export;
pub mod filter;
pub mod ingest;
pub mod normalize;
pub mod parser;
pub mod pipeline;
