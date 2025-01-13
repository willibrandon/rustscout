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
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Custom error types for search operations
#[derive(Error, Debug)]
pub enum SearchError {
    /// File not found error
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    /// Permission denied error
    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },

    /// Invalid pattern error
    #[error("Invalid pattern: {message}")]
    InvalidPattern { message: String },

    /// File too large error
    #[error("File too large: {path} ({size} bytes)")]
    FileTooLarge { path: PathBuf, size: u64 },

    /// Thread pool error
    #[error("Thread pool error: {message}")]
    ThreadPoolError { message: String },

    /// I/O error
    #[error(transparent)]
    IoError(#[from] io::Error),

    /// Invalid file encoding error
    #[error("Invalid file encoding: {path}")]
    InvalidEncoding { path: PathBuf },
}

/// Type alias for Results that may return a SearchError
pub type SearchResult<T> = Result<T, SearchError>;

impl SearchError {
    /// Creates a new FileNotFound error
    pub fn file_not_found(path: &Path) -> Self {
        SearchError::FileNotFound {
            path: path.to_path_buf(),
        }
    }

    /// Creates a new PermissionDenied error
    pub fn permission_denied(path: &Path) -> Self {
        SearchError::PermissionDenied {
            path: path.to_path_buf(),
        }
    }

    /// Creates a new InvalidPattern error
    pub fn invalid_pattern<S: Into<String>>(message: S) -> Self {
        SearchError::InvalidPattern {
            message: message.into(),
        }
    }

    /// Creates a new FileTooLarge error
    pub fn file_too_large(path: &Path, size: u64) -> Self {
        SearchError::FileTooLarge {
            path: path.to_path_buf(),
            size,
        }
    }

    /// Creates a new ThreadPoolError error
    pub fn thread_pool_error<S: Into<String>>(message: S) -> Self {
        SearchError::ThreadPoolError {
            message: message.into(),
        }
    }

    /// Creates a new InvalidEncoding error
    pub fn invalid_encoding(path: &Path) -> Self {
        SearchError::InvalidEncoding {
            path: path.to_path_buf(),
        }
    }

    /// Returns true if this is a FileNotFound error
    pub fn is_not_found(&self) -> bool {
        matches!(self, SearchError::FileNotFound { .. })
    }

    /// Returns true if this is a PermissionDenied error
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, SearchError::PermissionDenied { .. })
    }

    /// Returns true if this is an InvalidPattern error
    pub fn is_invalid_pattern(&self) -> bool {
        matches!(self, SearchError::InvalidPattern { .. })
    }

    /// Returns true if this is a FileTooLarge error
    pub fn is_file_too_large(&self) -> bool {
        matches!(self, SearchError::FileTooLarge { .. })
    }

    /// Returns true if this is a ThreadPoolError error
    pub fn is_thread_pool_error(&self) -> bool {
        matches!(self, SearchError::ThreadPoolError { .. })
    }

    /// Returns true if this is an InvalidEncoding error
    pub fn is_invalid_encoding(&self) -> bool {
        matches!(self, SearchError::InvalidEncoding { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_error_creation() {
        let path = PathBuf::from("test.txt");

        let err = SearchError::file_not_found(&path);
        assert!(err.is_not_found());

        let err = SearchError::permission_denied(&path);
        assert!(err.is_permission_denied());

        let err = SearchError::invalid_pattern("invalid[regex");
        assert!(err.is_invalid_pattern());

        let err = SearchError::file_too_large(&path, 1024);
        assert!(err.is_file_too_large());

        let err = SearchError::thread_pool_error("thread error");
        assert!(err.is_thread_pool_error());

        let err = SearchError::invalid_encoding(&path);
        assert!(err.is_invalid_encoding());
    }

    #[test]
    fn test_error_messages() {
        let path = PathBuf::from("test.txt");

        let err = SearchError::file_not_found(&path);
        assert_eq!(err.to_string(), "File not found: test.txt");

        let err = SearchError::permission_denied(&path);
        assert_eq!(err.to_string(), "Permission denied: test.txt");

        let err = SearchError::invalid_pattern("bad pattern");
        assert_eq!(err.to_string(), "Invalid pattern: bad pattern");

        let err = SearchError::file_too_large(&path, 1024);
        assert_eq!(err.to_string(), "File too large: test.txt (1024 bytes)");

        let err = SearchError::thread_pool_error("thread error");
        assert_eq!(err.to_string(), "Thread pool error: thread error");

        let err = SearchError::invalid_encoding(&path);
        assert_eq!(err.to_string(), "Invalid file encoding: test.txt");
    }
}
