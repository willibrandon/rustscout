use std::io;
use serde_json;
use serde_yaml;
use thiserror;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

impl SearchError {
    pub fn config_error(msg: impl Into<String>) -> Self {
        SearchError::ConfigError(msg.into())
    }
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::IoError(e) => write!(f, "IO error: {}", e),
            SearchError::JsonError(e) => write!(f, "JSON error: {}", e),
            SearchError::YamlError(e) => write!(f, "YAML error: {}", e),
            SearchError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for SearchError {} 