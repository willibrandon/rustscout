pub mod config;
pub mod errors;
pub mod filters;
pub mod results;
pub mod search;

pub use config::SearchConfig;
pub use errors::{SearchError, SearchResult};
pub use results::{FileResult, Match, SearchResult as SearchOutput};
