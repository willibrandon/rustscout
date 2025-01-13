use std::num::NonZeroUsize;
use std::path::PathBuf;

/// Configuration for the search operation
#[derive(Debug, Clone)]
pub struct Config {
    /// The pattern to search for (regex)
    pub pattern: String,
    
    /// The root directory to start searching from
    pub root_path: PathBuf,
    
    /// Number of threads to use for parallel search
    pub thread_count: NonZeroUsize,
    
    /// File patterns to ignore (e.g. *.git/*)
    pub ignore_patterns: Vec<String>,
    
    /// Whether to only show statistics instead of matches
    pub stats_only: bool,
    
    /// File extensions to include in the search
    pub file_extensions: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            pattern: String::new(),
            root_path: PathBuf::from("."),
            thread_count: NonZeroUsize::new(num_cpus::get()).unwrap(),
            ignore_patterns: vec![],
            stats_only: false,
            file_extensions: None,
        }
    }
}

impl Config {
    /// Creates a new configuration with the given pattern and root path
    pub fn new(pattern: String, root_path: PathBuf) -> Self {
        Config {
            pattern,
            root_path,
            ..Default::default()
        }
    }

    /// Builder method to set the number of threads
    pub fn with_thread_count(mut self, count: NonZeroUsize) -> Self {
        self.thread_count = count;
        self
    }

    /// Builder method to set ignore patterns
    pub fn with_ignore_patterns(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns = patterns;
        self
    }

    /// Builder method to set stats only mode
    pub fn with_stats_only(mut self, stats_only: bool) -> Self {
        self.stats_only = stats_only;
        self
    }

    /// Builder method to set file extensions to include
    pub fn with_file_extensions(mut self, extensions: Vec<String>) -> Self {
        self.file_extensions = Some(extensions);
        self
    }
}
