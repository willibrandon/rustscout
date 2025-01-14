use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::cache::ChangeDetectionStrategy;
use crate::errors::{SearchError, SearchResult};

/// Configuration for search operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchConfig {
    /// Search patterns (supports multiple patterns)
    pub patterns: Vec<String>,
    /// Legacy single pattern field (for backward compatibility)
    pub pattern: String,
    /// Root directory to search in
    pub root_path: PathBuf,
    /// File extensions to include (None means all)
    pub file_extensions: Option<Vec<String>>,
    /// Patterns to ignore
    pub ignore_patterns: Vec<String>,
    /// Only show statistics, not matches
    pub stats_only: bool,
    /// Number of threads to use
    pub thread_count: NonZeroUsize,
    /// Log level
    pub log_level: String,
    /// Number of context lines before matches
    pub context_before: usize,
    /// Number of context lines after matches
    pub context_after: usize,
    /// Whether to use incremental search
    pub incremental: bool,
    /// Path to the cache file
    pub cache_path: Option<PathBuf>,
    /// Strategy for detecting changes
    pub cache_strategy: ChangeDetectionStrategy,
    /// Maximum cache size in bytes
    pub max_cache_size: Option<u64>,
    /// Whether to use compression for cache
    pub use_compression: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            pattern: String::new(),
            root_path: PathBuf::from("."),
            file_extensions: None,
            ignore_patterns: Vec::new(),
            stats_only: false,
            thread_count: NonZeroUsize::new(4).unwrap(),
            log_level: "info".to_string(),
            context_before: 0,
            context_after: 0,
            incremental: false,
            cache_path: None,
            cache_strategy: ChangeDetectionStrategy::Auto,
            max_cache_size: None,
            use_compression: false,
        }
    }
}

impl SearchConfig {
    /// Loads configuration from a file
    pub fn load_from(path: impl AsRef<Path>) -> SearchResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SearchError::config_error(format!("Failed to read config: {}", e)))?;

        serde_yaml::from_str(&content)
            .map_err(|e| SearchError::config_error(format!("Failed to parse config: {}", e)))
    }

    /// Gets the default cache path
    pub fn default_cache_path(&self) -> PathBuf {
        self.root_path.join(".rustscout").join("cache.json")
    }

    /// Gets the effective cache path
    pub fn get_cache_path(&self) -> PathBuf {
        self.cache_path
            .clone()
            .unwrap_or_else(|| self.default_cache_path())
    }

    pub fn merge_with_cli(&mut self, cli: &SearchConfig) {
        if !cli.patterns.is_empty() {
            self.patterns = cli.patterns.clone();
        }
        if !cli.pattern.is_empty() {
            self.pattern = cli.pattern.clone();
        }
        if cli.root_path != PathBuf::from(".") {
            self.root_path = cli.root_path.clone();
        }
        if cli.file_extensions.is_some() {
            self.file_extensions = cli.file_extensions.clone();
        }
        if !cli.ignore_patterns.is_empty() {
            self.ignore_patterns = cli.ignore_patterns.clone();
        }
        if cli.stats_only {
            self.stats_only = true;
        }
        if cli.thread_count.get() != 4 {
            self.thread_count = cli.thread_count;
        }
        if cli.log_level != "info" {
            self.log_level = cli.log_level.clone();
        }
        if cli.context_before != 0 {
            self.context_before = cli.context_before;
        }
        if cli.context_after != 0 {
            self.context_after = cli.context_after;
        }
        if cli.incremental {
            self.incremental = true;
        }
        if cli.cache_path.is_some() {
            self.cache_path = cli.cache_path.clone();
        }
        if cli.cache_strategy != ChangeDetectionStrategy::Auto {
            self.cache_strategy = cli.cache_strategy;
        }
        if cli.max_cache_size.is_some() {
            self.max_cache_size = cli.max_cache_size;
        }
        if cli.use_compression {
            self.use_compression = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_default_values() {
        let config = SearchConfig::default();
        assert!(config.patterns.is_empty());
        assert!(config.pattern.is_empty());
        assert_eq!(config.root_path, PathBuf::from("."));
        assert!(config.file_extensions.is_none());
        assert!(config.ignore_patterns.is_empty());
        assert!(!config.stats_only);
        assert_eq!(config.thread_count.get(), 4);
        assert_eq!(config.log_level, "info");
        assert_eq!(config.context_before, 0);
        assert_eq!(config.context_after, 0);
        assert!(!config.incremental);
        assert!(config.cache_path.is_none());
        assert_eq!(config.cache_strategy, ChangeDetectionStrategy::Auto);
        assert!(config.max_cache_size.is_none());
        assert!(!config.use_compression);
    }

    #[test]
    fn test_load_config_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let config_path = dir.path().join("config.yaml");

        let config_content = r#"
pattern: test
patterns: []
root_path: .
file_extensions: null
ignore_patterns: []
stats_only: false
thread_count: 4
log_level: info
context_before: 0
context_after: 0
incremental: false
cache_path: null
cache_strategy: Auto
max_cache_size: null
use_compression: false
"#;
        fs::write(&config_path, config_content)?;

        let config = SearchConfig::load_from(&config_path)?;
        assert_eq!(config.pattern, "test");
        assert!(config.patterns.is_empty());
        assert_eq!(config.root_path, PathBuf::from("."));
        assert!(config.file_extensions.is_none());
        assert!(config.ignore_patterns.is_empty());
        assert!(!config.stats_only);
        assert_eq!(config.thread_count.get(), 4);
        assert_eq!(config.log_level, "info");
        assert_eq!(config.context_before, 0);
        assert_eq!(config.context_after, 0);
        assert!(!config.incremental);
        assert!(config.cache_path.is_none());
        assert_eq!(config.cache_strategy, ChangeDetectionStrategy::Auto);
        assert!(config.max_cache_size.is_none());
        assert!(!config.use_compression);

        Ok(())
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = Path::new("nonexistent.yaml");
        let result = SearchConfig::load_from(path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Failed to read config"),
            "Error message was: {}",
            err
        );
    }

    #[test]
    fn test_invalid_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("invalid.yaml");
        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "invalid: yaml: content").unwrap();

        assert!(SearchConfig::load_from(config_path).is_err());
    }

    #[test]
    fn test_merge_with_cli() {
        let mut config = SearchConfig::default();
        let cli = SearchConfig {
            patterns: vec!["TODO".to_string()],
            pattern: "FIXME".to_string(),
            root_path: PathBuf::from("/search"),
            file_extensions: Some(vec!["rs".to_string()]),
            ignore_patterns: vec!["target".to_string()],
            stats_only: true,
            thread_count: NonZeroUsize::new(4).unwrap(),
            log_level: "debug".to_string(),
            context_before: 2,
            context_after: 2,
            incremental: true,
            cache_path: Some(PathBuf::from("/cache")),
            cache_strategy: ChangeDetectionStrategy::GitStatus,
            max_cache_size: Some(104857600),
            use_compression: true,
            ..SearchConfig::default()
        };

        config.merge_with_cli(&cli);

        assert_eq!(config.patterns, vec!["TODO"]);
        assert_eq!(config.pattern, "FIXME");
        assert_eq!(config.root_path, PathBuf::from("/search"));
        assert_eq!(config.file_extensions, Some(vec!["rs".to_string()]));
        assert_eq!(config.ignore_patterns, vec!["target"]);
        assert!(config.stats_only);
        assert_eq!(config.thread_count.get(), 4);
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.context_before, 2);
        assert_eq!(config.context_after, 2);
        assert!(config.incremental);
        assert_eq!(config.cache_path, Some(PathBuf::from("/cache")));
        assert_eq!(config.cache_strategy, ChangeDetectionStrategy::GitStatus);
        assert_eq!(config.max_cache_size, Some(104857600));
        assert!(config.use_compression);
    }
}
