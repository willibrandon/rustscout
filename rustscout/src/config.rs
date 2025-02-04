use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::cache::ChangeDetectionStrategy;
use crate::errors::{SearchError, SearchResult};
use crate::search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode};

/// Controls how invalid UTF-8 sequences are handled
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EncodingMode {
    /// Fail immediately on invalid UTF-8 (default for code search)
    FailFast,
    /// Replace invalid UTF-8 sequences with the replacement character ()
    Lossy,
}

impl Default for EncodingMode {
    fn default() -> Self {
        Self::FailFast // Default to strict UTF-8 for code search
    }
}

/// Configuration for search operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchConfig {
    /// Pattern definitions with boundary settings (new field)
    #[serde(default)]
    pub pattern_definitions: Vec<PatternDefinition>,
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
    /// How to handle invalid UTF-8 sequences
    #[serde(default)]
    pub encoding_mode: EncodingMode,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            pattern_definitions: Vec::new(),
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
            encoding_mode: EncodingMode::default(),
        }
    }
}

impl SearchConfig {
    /// Creates a new SearchConfig with a single pattern
    pub fn new_with_pattern(text: String, is_regex: bool, boundary_mode: WordBoundaryMode) -> Self {
        let mut config = Self::default();
        config.pattern_definitions.push(PatternDefinition {
            text,
            is_regex,
            boundary_mode,
            hyphen_mode: HyphenMode::default(),
        });
        config
    }

    /// Gets all pattern definitions, including those from legacy fields
    pub fn get_pattern_definitions(&self) -> Vec<PatternDefinition> {
        self.pattern_definitions.clone()
    }

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
        // Merge pattern definitions first
        if !cli.pattern_definitions.is_empty() {
            self.pattern_definitions = cli.pattern_definitions.clone();
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
        if cli.encoding_mode != EncodingMode::default() {
            self.encoding_mode = cli.encoding_mode;
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
        assert!(config.pattern_definitions.is_empty());
        assert_eq!(config.root_path, PathBuf::from("."));
        assert_eq!(config.file_extensions, None);
        assert!(config.ignore_patterns.is_empty());
        assert!(!config.stats_only);
        assert_eq!(config.thread_count, NonZeroUsize::new(4).unwrap());
        assert_eq!(config.log_level, "info");
        assert_eq!(config.context_before, 0);
        assert_eq!(config.context_after, 0);
        assert!(!config.incremental);
        assert_eq!(config.cache_path, None);
        assert_eq!(config.cache_strategy, ChangeDetectionStrategy::Auto);
        assert_eq!(config.max_cache_size, None);
        assert!(!config.use_compression);
        assert_eq!(config.encoding_mode, EncodingMode::default());
    }

    #[test]
    fn test_new_with_pattern() {
        let config =
            SearchConfig::new_with_pattern("test".to_string(), false, WordBoundaryMode::WholeWords);
        assert_eq!(config.pattern_definitions.len(), 1);
        assert_eq!(config.pattern_definitions[0].text, "test");
        assert!(!config.pattern_definitions[0].is_regex);
        assert_eq!(
            config.pattern_definitions[0].boundary_mode,
            WordBoundaryMode::WholeWords
        );
    }

    #[test]
    fn test_get_pattern_definitions() {
        let mut config = SearchConfig::default();
        config.pattern_definitions = vec![
            PatternDefinition {
                text: "test1".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::WholeWords,
                hyphen_mode: HyphenMode::default(),
            },
            PatternDefinition {
                text: "test2".to_string(),
                is_regex: true,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::default(),
            },
        ];

        let defs = config.get_pattern_definitions();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].text, "test1");
        assert!(!defs[0].is_regex);
        assert_eq!(defs[0].boundary_mode, WordBoundaryMode::WholeWords);
        assert_eq!(defs[1].text, "test2");
        assert!(defs[1].is_regex);
        assert_eq!(defs[1].boundary_mode, WordBoundaryMode::None);
    }

    #[test]
    fn test_merge_with_cli() {
        let mut config = SearchConfig::default();
        config.pattern_definitions = vec![PatternDefinition {
            text: "test1".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::WholeWords,
            hyphen_mode: HyphenMode::default(),
        }];

        let mut cli_config = SearchConfig::default();
        cli_config.pattern_definitions = vec![PatternDefinition {
            text: "test2".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_mode: HyphenMode::default(),
        }];

        config.merge_with_cli(&cli_config);

        assert_eq!(config.pattern_definitions.len(), 1);
        assert_eq!(config.pattern_definitions[0].text, "test2");
        assert!(config.pattern_definitions[0].is_regex);
        assert_eq!(
            config.pattern_definitions[0].boundary_mode,
            WordBoundaryMode::None
        );
    }

    #[test]
    fn test_load_config_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let config_path = dir.path().join("config.yaml");

        let config_content = r#"
pattern_definitions:
  - text: test
    is_regex: false
    boundary_mode: None
    hyphen_mode: Joining
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
        assert_eq!(config.pattern_definitions.len(), 1);
        assert_eq!(config.pattern_definitions[0].text, "test");
        assert!(!config.pattern_definitions[0].is_regex);
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
}
