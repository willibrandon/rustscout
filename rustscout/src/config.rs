use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use config::{Config as ConfigBuilder, ConfigError, File};

/// Configuration for the search operation, demonstrating Rust's strong typing
/// compared to .NET's optional configuration pattern.
///
/// # Configuration Locations
///
/// The configuration can be loaded from multiple locations in order of precedence:
/// 1. Custom config file specified via `--config` flag
/// 2. Local `.rustscout.yaml` in the current directory
/// 3. Global `$HOME/.config/rustscout/config.yaml`
///
/// # Configuration Format
///
/// The configuration uses YAML format. Example:
/// ```yaml
/// # Search pattern (supports regex)
/// pattern: "TODO|FIXME"
///
/// # Root directory to search in
/// root_path: "."
///
/// # File extensions to include
/// file_extensions:
///   - "rs"
///   - "toml"
///
/// # Patterns to ignore (glob syntax)
/// ignore_patterns:
///   - "target/**"
///   - ".git/**"
///
/// # Show only statistics
/// stats_only: false
///
/// # Thread count (default: CPU cores)
/// thread_count: 4
///
/// # Log level (trace, debug, info, warn, error)
/// log_level: "info"
/// ```
///
/// # CLI Integration
///
/// When using the CLI, command-line arguments take precedence over config file values.
/// The merging behavior is defined in the `merge_with_cli` method.
///
/// # Error Handling
///
/// Configuration errors are handled using Rust's Result type with ConfigError:
/// ```rust,ignore
/// match SearchConfig::load() {
///     Ok(config) => // Use config,
///     Err(e) => eprintln!("Failed to load config: {}", e)
/// }
/// ```
///
/// # Rust vs .NET Configuration
///
/// .NET's IConfiguration pattern:
/// ```csharp
/// public class SearchOptions
/// {
///     public string Pattern { get; set; }
///     public string RootPath { get; set; }
///     public List<string> FileExtensions { get; set; }
///     // No compile-time guarantees for null values
/// }
/// ```
///
/// Rust's strongly-typed configuration:
/// ```rust,ignore
/// #[derive(Deserialize)]
/// pub struct SearchConfig {
///     pub pattern: String,
///     pub root_path: PathBuf,
///     pub file_extensions: Option<Vec<String>>,
///     // Option explicitly handles missing values
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// The search pattern (supports regex)
    pub pattern: String,

    /// Root directory to start search from
    pub root_path: PathBuf,

    /// Optional list of file extensions to include (e.g., ["rs", "toml"])
    /// If None, all file extensions are included
    #[serde(default)]
    pub file_extensions: Option<Vec<String>>,

    /// Patterns to ignore (supports glob syntax)
    /// Examples:
    /// - "target/**": Ignore everything under target/
    /// - "**/*.min.js": Ignore all minified JS files
    /// - ".git/*": Ignore direct children of .git/
    #[serde(default)]
    pub ignore_patterns: Vec<String>,

    /// Whether to only show statistics instead of individual matches
    /// When true, only displays total match count and file count
    #[serde(default)]
    pub stats_only: bool,

    /// Number of threads to use for searching
    /// Defaults to the number of CPU cores if not specified
    #[serde(default = "default_thread_count")]
    pub thread_count: NonZeroUsize,

    /// Log level for output (trace, debug, info, warn, error)
    /// - trace: Most verbose, shows all details
    /// - debug: Shows detailed progress
    /// - info: Shows important progress
    /// - warn: Shows only warnings
    /// - error: Shows only errors
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_thread_count() -> NonZeroUsize {
    NonZeroUsize::new(num_cpus::get()).unwrap()
}

fn default_log_level() -> String {
    "warn".to_string()
}

impl SearchConfig {
    /// Loads configuration from the default locations
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(None)
    }

    /// Loads configuration from a specific file
    pub fn load_from(config_path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut builder = ConfigBuilder::builder();

        // Default config locations
        let config_files = [
            // Global config
            dirs::config_dir().map(|p| p.join("rustscout/config.yaml")),
            // Local config
            Some(PathBuf::from(".rustscout.yaml")),
            // Custom config
            config_path.map(PathBuf::from),
        ];

        // Add existing config files
        for path in config_files.iter().flatten() {
            if path.exists() {
                builder = builder.add_source(File::from(path.as_path()));
            }
        }

        // Build and deserialize
        builder.build()?.try_deserialize()
    }

    /// Merges CLI arguments with configuration file values
    pub fn merge_with_cli(mut self, cli_config: SearchConfig) -> Self {
        // CLI values take precedence over config file values
        if !cli_config.pattern.is_empty() {
            self.pattern = cli_config.pattern;
        }
        if cli_config.root_path != PathBuf::from(".") {
            self.root_path = cli_config.root_path;
        }
        if cli_config.file_extensions.is_some() {
            self.file_extensions = cli_config.file_extensions;
        }
        if !cli_config.ignore_patterns.is_empty() {
            self.ignore_patterns = cli_config.ignore_patterns;
        }
        if cli_config.stats_only {
            self.stats_only = true;
        }
        if cli_config.thread_count != default_thread_count() {
            self.thread_count = cli_config.thread_count;
        }
        if cli_config.log_level != default_log_level() {
            self.log_level = cli_config.log_level;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_load_config_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.yaml");
        let config_content = r#"
            pattern: "TODO|FIXME"
            root_path: "src"
            file_extensions: ["rs", "toml"]
            ignore_patterns: ["target/*"]
            stats_only: true
            thread_count: 4
            log_level: "debug"
        "#;

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        let config = SearchConfig::load_from(Some(&config_path)).unwrap();
        assert_eq!(config.pattern, "TODO|FIXME");
        assert_eq!(config.root_path, PathBuf::from("src"));
        assert_eq!(config.file_extensions, Some(vec!["rs".to_string(), "toml".to_string()]));
        assert_eq!(config.ignore_patterns, vec!["target/*".to_string()]);
        assert!(config.stats_only);
        assert_eq!(config.thread_count, NonZeroUsize::new(4).unwrap());
        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn test_merge_with_cli() {
        let config_file = SearchConfig {
            pattern: "TODO".to_string(),
            root_path: PathBuf::from("src"),
            file_extensions: Some(vec!["rs".to_string()]),
            ignore_patterns: vec!["target/*".to_string()],
            stats_only: false,
            thread_count: NonZeroUsize::new(4).unwrap(),
            log_level: "warn".to_string(),
        };

        let cli_config = SearchConfig {
            pattern: "FIXME".to_string(),
            root_path: PathBuf::from("tests"),
            file_extensions: None,
            ignore_patterns: vec!["*.tmp".to_string()],
            stats_only: true,
            thread_count: NonZeroUsize::new(8).unwrap(),
            log_level: "debug".to_string(),
        };

        let merged = config_file.merge_with_cli(cli_config);
        assert_eq!(merged.pattern, "FIXME"); // CLI value
        assert_eq!(merged.root_path, PathBuf::from("tests")); // CLI value
        assert_eq!(merged.file_extensions, Some(vec!["rs".to_string()])); // File value (CLI None)
        assert_eq!(merged.ignore_patterns, vec!["*.tmp".to_string()]); // CLI value
        assert!(merged.stats_only); // CLI value
        assert_eq!(merged.thread_count, NonZeroUsize::new(8).unwrap()); // CLI value
        assert_eq!(merged.log_level, "debug"); // CLI value
    }

    #[test]
    fn test_default_values() {
        let config_content = r#"
            pattern: "test"
            root_path: "."
        "#;

        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.yaml");
        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        let config = SearchConfig::load_from(Some(&config_path)).unwrap();
        assert_eq!(config.pattern, "test");
        assert_eq!(config.root_path, PathBuf::from("."));
        assert_eq!(config.file_extensions, None);
        assert!(config.ignore_patterns.is_empty());
        assert!(!config.stats_only);
        assert_eq!(config.thread_count, NonZeroUsize::new(num_cpus::get()).unwrap());
        assert_eq!(config.log_level, "warn");
    }

    #[test]
    fn test_invalid_config() {
        let config_content = r#"
            pattern: 123  # Should be string
            root_path: []  # Should be string
            thread_count: "invalid"  # Should be number
        "#;

        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.yaml");
        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        let result = SearchConfig::load_from(Some(&config_path));
        assert!(result.is_err(), "Expected error loading invalid config");
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = SearchConfig::load_from(Some(Path::new("nonexistent.yaml")));
        assert!(result.is_err());
    }
}