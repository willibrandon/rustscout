pub mod config;
pub mod search;
pub mod filters;
pub mod results;
pub mod errors;

pub use config::Config;
pub use results::SearchResult;

/// Re-export common types for convenience
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::results::SearchResult;
    pub use crate::errors::RTraceError;
}
