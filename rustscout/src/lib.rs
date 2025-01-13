//! High-performance concurrent code search library
//!
//! This library provides functionality for searching code repositories
//! with parallel processing capabilities.

pub mod config;
pub mod errors;
pub mod filters;
pub mod results;
pub mod search;

pub use config::SearchConfig;
pub use errors::{SearchError, SearchResult};
pub use results::{FileResult, Match, SearchResult as SearchOutput};
pub use search::search;
