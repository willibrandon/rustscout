//! High-performance concurrent code search library
//!
//! This library provides functionality for searching code repositories
//! with parallel processing capabilities.

pub mod cache;
pub mod config;
pub mod errors;
pub mod filters;
pub mod metrics;
pub mod replace;
pub mod results;
pub mod search;

pub use cache::{
    ChangeDetectionStrategy, ChangeDetector, ChangeStatus, FileChangeInfo, FileSignatureDetector,
    GitStatusDetector, IncrementalCache,
};
pub use config::SearchConfig;
pub use errors::{SearchError, SearchResult};
pub use glob::Pattern;
pub use metrics::MemoryMetrics;
pub use replace::{FileReplacementPlan, ReplacementConfig, ReplacementSet, ReplacementTask};
pub use results::{FileResult, Match, SearchResult as SearchResultType};
pub use search::search;
