/// This module implements search result types, demonstrating key differences between
/// Rust's ownership system and .NET's reference types.
///
/// # Rust Ownership vs .NET References
///
/// The handling of search results highlights fundamental differences between Rust and .NET:
///
/// 1. **Memory Management**
///    .NET class (reference type):
///    ```csharp
///    public class SearchResult {
///        public List<Match> Matches { get; set; }
///        // Garbage collector handles cleanup
///        // Risk of memory leaks through circular references
///        // No explicit control over when cleanup occurs
///    }
///    ```
///    
///    Rust struct (owned type):
///    ```rust,ignore
///    pub struct SearchResult {
///        pub matches: Vec<Match>,
///        // Automatically cleaned up when SearchResult is dropped
///        // No garbage collector needed
///        // Deterministic cleanup
///    }
///    ```
///
/// 2. **Reference Safety**
///    .NET can have null references and race conditions:
///    ```csharp
///    public class SearchEngine {
///        private SearchResult _lastResult; // Can be null
///        
///        public void ProcessResult(SearchResult result) {
///            _lastResult = result; // Possible race condition
///            // No compile-time guarantees about thread safety
///        }
///    }
///    ```
///    
///    Rust enforces safety at compile time:
///    ```rust,ignore
///    pub struct SearchEngine {
///        last_result: Option<SearchResult>, // Explicit optional value
///        
///        pub fn process_result(&mut self, result: SearchResult) {
///            self.last_result = Some(result); // Ownership transferred
///            // Compiler ensures exclusive access through &mut
///        }
///    }
///    ```
///
/// 3. **Data Sharing**
///    .NET allows multiple mutable references:
///    ```csharp
///    var result = new SearchResult();
///    var ref1 = result; // Reference
///    var ref2 = result; // Another reference to same data
///    // Both can modify data simultaneously
///    ```
///    
///    Rust enforces borrowing rules:
///    ```rust,ignore
///    let result = SearchResult::new();
///    let ref1 = &result; // Immutable borrow
///    let ref2 = &result; // Multiple immutable borrows OK
///    let mut_ref = &mut result; // ERROR: Can't borrow mutably
///                               // while immutable borrows exist
///    ```
///
/// 4. **Clone vs Copy**
///    .NET reference types are implicitly shared:
///    ```csharp
///    var result1 = new SearchResult();
///    var result2 = result1; // Both reference same data
///    ```
///    
///    Rust requires explicit cloning for complex types:
///    ```rust,ignore
///    let result1 = SearchResult::new();
///    let result2 = result1.clone(); // Explicit deep copy
///    // result1 moved here if not explicitly cloned
///    ```
///
/// The types in this module use Rust's ownership system to provide memory safety
/// and thread safety guarantees at compile time, preventing common issues that
/// can occur in .NET applications.
use std::path::PathBuf;

/// Represents a single match in a file
#[derive(Debug, Clone)]
pub struct Match {
    /// The line number where the match was found
    pub line_number: usize,
    /// The content of the line containing the match
    pub line_content: String,
    /// The start position of the match within the line
    pub start: usize,
    /// The end position of the match within the line
    pub end: usize,
    /// Lines before the match for context
    pub context_before: Vec<(usize, String)>,
    /// Lines after the match for context
    pub context_after: Vec<(usize, String)>,
}

/// Represents all matches found in a single file
#[derive(Debug, Clone)]
pub struct FileResult {
    /// The path to the file
    pub path: PathBuf,
    /// All matches found in the file
    pub matches: Vec<Match>,
}

/// Represents the complete search results
#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    /// Results per file
    pub file_results: Vec<FileResult>,
    /// Total number of matches found
    pub total_matches: usize,
    /// Total number of files searched
    pub files_searched: usize,
    /// Total number of files with matches
    pub files_with_matches: usize,
}

impl SearchResult {
    /// Creates a new empty search result
    pub fn new() -> Self {
        Default::default()
    }

    /// Adds a file result to the search results
    pub fn add_file_result(&mut self, file_result: FileResult) {
        self.files_searched += 1;
        if !file_result.matches.is_empty() {
            self.total_matches += file_result.matches.len();
            self.files_with_matches += 1;
        }
        self.file_results.push(file_result);
    }

    /// Merges another search result into this one
    pub fn merge(&mut self, other: SearchResult) {
        self.total_matches += other.total_matches;
        self.files_searched += other.files_searched;
        self.files_with_matches += other.files_with_matches;
        self.file_results.extend(other.file_results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_creation() {
        let m = Match {
            line_number: 42,
            line_content: "Hello, world!".to_string(),
            start: 0,
            end: 5,
            context_before: vec![],
            context_after: vec![],
        };

        assert_eq!(m.line_number, 42);
        assert_eq!(m.line_content, "Hello, world!");
        assert_eq!(m.start, 0);
        assert_eq!(m.end, 5);
        assert_eq!(&m.line_content[m.start..m.end], "Hello");
    }

    #[test]
    fn test_file_result_creation() {
        let matches = vec![
            Match {
                line_number: 1,
                line_content: "Hello".to_string(),
                start: 0,
                end: 5,
                context_before: vec![],
                context_after: vec![],
            },
            Match {
                line_number: 2,
                line_content: "World Hello".to_string(),
                start: 6,
                end: 11,
                context_before: vec![],
                context_after: vec![],
            },
        ];

        let file_result = FileResult {
            path: PathBuf::from("test.txt"),
            matches,
        };

        assert_eq!(file_result.path, PathBuf::from("test.txt"));
        assert_eq!(file_result.matches.len(), 2);
        assert_eq!(file_result.matches[0].line_number, 1);
        assert_eq!(file_result.matches[1].line_number, 2);
    }

    #[test]
    fn test_search_result_new() {
        let result = SearchResult::new();
        assert_eq!(result.total_matches, 0);
        assert_eq!(result.files_searched, 0);
        assert_eq!(result.files_with_matches, 0);
        assert!(result.file_results.is_empty());
    }

    #[test]
    fn test_search_result_add_file_result() {
        let mut result = SearchResult::new();

        // Add a file with matches
        let file_result1 = FileResult {
            path: PathBuf::from("test1.txt"),
            matches: vec![
                Match {
                    line_number: 1,
                    line_content: "Hello".to_string(),
                    start: 0,
                    end: 5,
                    context_before: vec![],
                    context_after: vec![],
                },
                Match {
                    line_number: 2,
                    line_content: "Hello again".to_string(),
                    start: 0,
                    end: 5,
                    context_before: vec![],
                    context_after: vec![],
                },
            ],
        };
        result.add_file_result(file_result1);

        assert_eq!(result.total_matches, 2);
        assert_eq!(result.files_searched, 1);
        assert_eq!(result.files_with_matches, 1);

        // Add a file without matches
        let file_result2 = FileResult {
            path: PathBuf::from("test2.txt"),
            matches: vec![],
        };
        result.add_file_result(file_result2);

        assert_eq!(result.total_matches, 2); // Unchanged
        assert_eq!(result.files_searched, 2); // Incremented
        assert_eq!(result.files_with_matches, 1); // Unchanged
    }

    #[test]
    fn test_search_result_merge() {
        let mut result1 = SearchResult::new();
        let mut result2 = SearchResult::new();

        // Add results to first SearchResult
        result1.add_file_result(FileResult {
            path: PathBuf::from("test1.txt"),
            matches: vec![Match {
                line_number: 1,
                line_content: "Hello".to_string(),
                start: 0,
                end: 5,
                context_before: vec![],
                context_after: vec![],
            }],
        });

        // Add results to second SearchResult
        result2.add_file_result(FileResult {
            path: PathBuf::from("test2.txt"),
            matches: vec![
                Match {
                    line_number: 1,
                    line_content: "World".to_string(),
                    start: 0,
                    end: 5,
                    context_before: vec![],
                    context_after: vec![],
                },
                Match {
                    line_number: 2,
                    line_content: "Hello".to_string(),
                    start: 0,
                    end: 5,
                    context_before: vec![],
                    context_after: vec![],
                },
            ],
        });

        // Add a file without matches to result2
        result2.add_file_result(FileResult {
            path: PathBuf::from("test3.txt"),
            matches: vec![],
        });

        // Merge results
        result1.merge(result2);

        assert_eq!(result1.total_matches, 3);
        assert_eq!(result1.files_searched, 3);
        assert_eq!(result1.files_with_matches, 2);
        assert_eq!(result1.file_results.len(), 3);

        // Verify file paths are preserved
        assert!(result1
            .file_results
            .iter()
            .any(|fr| fr.path == PathBuf::from("test1.txt")));
        assert!(result1
            .file_results
            .iter()
            .any(|fr| fr.path == PathBuf::from("test2.txt")));
        assert!(result1
            .file_results
            .iter()
            .any(|fr| fr.path == PathBuf::from("test3.txt")));
    }

    #[test]
    fn test_search_result_empty_merge() {
        let mut result1 = SearchResult::new();
        let result2 = SearchResult::new();

        // Add some results to result1
        result1.add_file_result(FileResult {
            path: PathBuf::from("test.txt"),
            matches: vec![Match {
                line_number: 1,
                line_content: "Hello".to_string(),
                start: 0,
                end: 5,
                context_before: vec![],
                context_after: vec![],
            }],
        });

        let initial_matches = result1.total_matches;
        let initial_files = result1.files_searched;
        let initial_files_with_matches = result1.files_with_matches;

        // Merge with empty result
        result1.merge(result2);

        // Verify nothing changed
        assert_eq!(result1.total_matches, initial_matches);
        assert_eq!(result1.files_searched, initial_files);
        assert_eq!(result1.files_with_matches, initial_files_with_matches);
    }
}
