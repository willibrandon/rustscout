/// This module defines custom error types for rustscout, demonstrating Rust's error handling
/// compared to .NET's exception system.
///
/// # Rust vs .NET Error Handling
///
/// .NET uses exceptions for error handling:
/// ```csharp
/// try {
///     var searcher = new FileSearcher();
///     searcher.Search(pattern);
/// } catch (FileNotFoundException ex) {
///     // Handle missing file
/// } catch (UnauthorizedAccessException ex) {
///     // Handle permission error
/// } catch (Exception ex) {
///     // Handle other errors
/// }
/// ```
///
/// Rust uses Result types with custom errors:
/// ```rust,ignore
/// match searcher.search(pattern) {
///     Ok(results) => // Process results,
///     Err(SearchError::FileNotFound(path)) => // Handle missing file,
///     Err(SearchError::PermissionDenied(path)) => // Handle permission error,
///     Err(e) => // Handle other errors
/// }
/// ```
///
/// # Benefits of Rust's Approach
///
/// 1. **Explicit Error Handling**
///    - .NET allows unchecked exceptions
///    - Rust requires explicit handling or propagation
///
/// 2. **Zero-Cost Abstractions**
///    - .NET exceptions have runtime overhead
///    - Rust's Result type has no runtime cost
///
/// 3. **Type Safety**
///    - .NET exceptions are discovered at runtime
///    - Rust errors are checked at compile time
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Result type for search operations
pub type SearchResult<T> = Result<T, SearchError>;

/// Errors that can occur during search operations
#[derive(Error, Debug)]
pub enum SearchError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("Cache error: {0}")]
    CacheError(String),
    #[error("Cache version mismatch: expected {current_version}, found {cache_version}")]
    CacheVersionMismatch {
        cache_version: String,
        current_version: String,
    },
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid UTF-8 in file {path}: {source}")]
    EncodingError {
        path: PathBuf,
        source: std::string::FromUtf8Error,
    },
}

/// Canonicalize the path and strip UNC prefixes so that
/// comparisons on Windows are consistent.
pub fn unify_path(original: &Path) -> PathBuf {
    let canonical = original
        .canonicalize()
        .unwrap_or_else(|_| original.to_path_buf());
    strip_unc_prefix(&canonical)
}

/// Strips the Windows UNC prefix (\\?\) from a path if present
fn strip_unc_prefix(p: &Path) -> PathBuf {
    let s = p.display().to_string();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p.to_path_buf()
    }
}

impl SearchError {
    pub fn file_not_found(path: impl Into<PathBuf>) -> Self {
        Self::FileNotFound(path.into())
    }

    pub fn permission_denied(path: impl Into<PathBuf>) -> Self {
        Self::PermissionDenied(path.into())
    }

    pub fn invalid_pattern(pattern: impl Into<String>) -> Self {
        Self::InvalidPattern(pattern.into())
    }

    pub fn cache_error(msg: impl Into<String>) -> Self {
        Self::CacheError(msg.into())
    }

    pub fn cache_version_mismatch(
        cache_version: impl Into<String>,
        current_version: impl Into<String>,
    ) -> Self {
        Self::CacheVersionMismatch {
            cache_version: cache_version.into(),
            current_version: current_version.into(),
        }
    }

    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }

    pub fn encoding_error(path: impl Into<PathBuf>, source: std::string::FromUtf8Error) -> Self {
        let path = path.into();
        let unified = unify_path(&path);
        Self::EncodingError {
            path: unified,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_error_creation() {
        let path = Path::new("test.txt");
        let err = SearchError::file_not_found(path);
        assert!(matches!(err, SearchError::FileNotFound(_)));

        let err = SearchError::permission_denied(path);
        assert!(matches!(err, SearchError::PermissionDenied(_)));

        let err = SearchError::invalid_pattern("Invalid regex");
        assert!(matches!(err, SearchError::InvalidPattern(_)));

        let err = SearchError::cache_error("Cache corrupted");
        assert!(matches!(err, SearchError::CacheError(_)));

        let err = SearchError::cache_version_mismatch("1.0.0".to_string(), "2.0.0".to_string());
        assert!(matches!(err, SearchError::CacheVersionMismatch { .. }));
    }

    #[test]
    fn test_error_messages() {
        let err = SearchError::cache_version_mismatch("1.0.0", "2.0.0");
        assert_eq!(
            err.to_string(),
            "Cache version mismatch: expected 2.0.0, found 1.0.0"
        );

        let err = SearchError::invalid_pattern("Invalid regex: missing closing brace".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid pattern: Invalid regex: missing closing brace"
        );

        let err = SearchError::config_error("Missing required field".to_string());
        assert_eq!(
            err.to_string(),
            "Configuration error: Missing required field"
        );

        let err = SearchError::file_not_found("test.txt");
        assert_eq!(err.to_string(), "File not found: test.txt");
    }
}
