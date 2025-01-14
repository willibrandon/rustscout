mod detector;

pub use detector::{
    create_detector, ChangeDetectionStrategy, ChangeDetector, ChangeStatus, FileChangeInfo,
    FileSignatureDetector, GitStatusDetector,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::errors::{SearchError, SearchResult};
use crate::results::Match;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct IncrementalCache {
    /// Maps absolute file paths to their cache entries
    pub files: HashMap<PathBuf, FileCacheEntry>,
    /// Metadata about the cache itself
    pub metadata: CacheMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileCacheEntry {
    /// File signature used to detect changes
    pub signature: FileSignature,
    /// Cached search results, if any
    pub search_results: Option<Vec<Match>>,
    /// When this entry was last accessed
    pub last_accessed: SystemTime,
    /// Number of times this entry has been accessed
    pub access_count: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FileSignature {
    pub mtime: SystemTime,
    pub size: u64,
    pub hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// RustScout version that created this cache
    pub version: String,
    /// When the cache was last updated
    pub last_search_timestamp: SystemTime,
    /// Cache hit rate (successful reuse of cached results)
    pub hit_rate: f64,
    /// Compression ratio when compression is enabled
    pub compression_ratio: Option<f64>,
    /// Files that change frequently
    pub frequently_changed: Vec<PathBuf>,
}

impl Default for CacheMetadata {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            last_search_timestamp: SystemTime::now(),
            hit_rate: 0.0,
            compression_ratio: None,
            frequently_changed: Vec::new(),
        }
    }
}

impl IncrementalCache {
    /// Creates a new empty cache
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            metadata: CacheMetadata {
                version: env!("CARGO_PKG_VERSION").to_string(),
                last_search_timestamp: SystemTime::now(),
                hit_rate: 0.0,
                compression_ratio: None,
                frequently_changed: Vec::new(),
            },
        }
    }

    /// Loads a cache from disk
    pub fn load_from(path: &Path) -> SearchResult<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(_) => return Ok(Self::new()),
        };

        match serde_json::from_slice(&data) {
            Ok(cache) => Ok(cache),
            Err(_) => {
                // Cache is corrupted, return a new one
                Ok(Self::new())
            }
        }
    }

    /// Saves the cache to disk
    pub fn save_to(&self, path: &Path) -> SearchResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SearchError::IoError)?;
        }

        // Write to a temporary file first
        let tmp_path = path.with_extension("tmp");
        let data =
            serde_json::to_vec_pretty(self).map_err(|e| SearchError::CacheError(e.to_string()))?;

        std::fs::write(&tmp_path, data).map_err(SearchError::IoError)?;

        // Atomically rename the temporary file
        std::fs::rename(&tmp_path, path).map_err(SearchError::IoError)?;

        Ok(())
    }

    /// Updates cache statistics after a search operation
    pub fn update_stats(&mut self, hits: usize, total: usize) {
        if total > 0 {
            self.metadata.hit_rate = hits as f64 / total as f64;
        }
        self.metadata.last_search_timestamp = SystemTime::now();
    }
}

impl FileCacheEntry {
    /// Creates a new cache entry
    pub fn new(signature: FileSignature) -> Self {
        Self {
            signature,
            search_results: None,
            last_accessed: SystemTime::now(),
            access_count: 0,
        }
    }

    /// Updates access statistics when this entry is used
    pub fn mark_accessed(&mut self) {
        self.last_accessed = SystemTime::now();
        self.access_count += 1;
    }
}
