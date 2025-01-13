pub mod config;
pub mod errors;
pub mod filters;
pub mod results;
pub mod search;

// Re-export commonly used types
pub use config::Config;
pub use errors::{SearchError, SearchResult};
pub use results::{FileResult, Match, SearchResult as SearchOutput};
