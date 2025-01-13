pub mod config;
pub mod errors;
pub mod filters;
pub mod results;
pub mod search;

pub use config::Config;
pub use results::SearchResult;

/// Re-export common types for convenience
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::errors::RTraceError;
    pub use crate::results::SearchResult;
}
