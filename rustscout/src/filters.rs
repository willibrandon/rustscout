/// This module implements file filtering functionality, demonstrating key differences between
/// Rust's trait system and .NET's interfaces.
///
/// # Rust Traits vs .NET Interfaces
///
/// While both traits and interfaces define contracts for types, they have important differences:
///
/// 1. **Default Implementations**
///    .NET interfaces (pre-C# 8.0):
///    ```csharp
///    public interface IFileFilter {
///        bool ShouldIncludeFile(string path);
///        // No default implementations possible
///    }
///    ```
///    
///    Rust traits:
///    ```rust,ignore
///    pub trait FileFilter {
///        fn should_include_file(&self, path: &Path) -> bool {
///            // Default implementation possible
///            !self.is_binary(path) && self.has_valid_extension(path)
///        }
///    }
///    ```
///
/// 2. **Coherence Rules**
///    .NET allows implementing interfaces anywhere:
///    ```csharp
///    // Can implement IFileFilter for any type, anywhere
///    public class ThirdPartyFilter : IFileFilter { }
///    ```
///    
///    Rust's orphan rule requires either the trait or type to be local:
///    ```rust,ignore
///    // Must own either the trait or the type to implement it
///    impl FileFilter for MyLocalType { }
///    impl MyLocalTrait for String { }
///    ```
///
/// 3. **Static Dispatch vs Dynamic Dispatch**
///    .NET interfaces always use virtual dispatch:
///    ```csharp
///    public void ProcessFile(IFileFilter filter) {
///        // Always uses virtual dispatch
///        if (filter.ShouldIncludeFile(path)) { }
///    }
///    ```
///    
///    Rust allows choosing between static and dynamic dispatch:
///    ```rust,ignore
///    // Static dispatch - resolved at compile time
///    fn process_file<T: FileFilter>(filter: &T) { }
///    
///    // Dynamic dispatch - resolved at runtime
///    fn process_file(filter: &dyn FileFilter) { }
///    ```
///
/// 4. **Multiple Trait Bounds**
///    .NET requires separate interface declarations:
///    ```csharp
///    public interface IFileFilter : IDisposable, ICloneable { }
///    ```
///    
///    Rust uses a more flexible trait bound syntax:
///    ```rust,ignore
///    fn process_file<T>(filter: &T)
///    where
///        T: FileFilter + Clone + Send + 'static
///    { }
///    ```
///
/// This module uses free functions instead of traits for simplicity, but the concepts
/// could be refactored into a trait-based design for more complex filtering requirements.
use glob::{MatchOptions, Pattern};
use std::path::Path;

/// Checks if a file should be included in the search based on its extension
pub fn has_valid_extension(path: &Path, extensions: &Option<Vec<String>>) -> bool {
    match extensions {
        None => true,
        Some(exts) => {
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    return exts.iter().any(|e| e.eq_ignore_ascii_case(ext_str));
                }
            }
            false
        }
    }
}

/// Convert `path` into a relative path (with forward slashes)
/// relative to `root_path`.
/// If `strip_prefix` fails (e.g. path isn't under root), fallback to the full path.
fn to_relative_slash_path(path: &Path, root_path: &Path) -> String {
    let rel = path.strip_prefix(root_path).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

/// Checks if a file should be ignored based on ignore patterns
///
/// Uses a simplified `.gitignore`-like syntax:
/// - If the pattern does not contain a slash, it matches only the final file name.
///   Example: `invalid.rs` matches any file named `invalid.rs` in any directory.
/// - If the pattern contains a slash, it is interpreted as a glob pattern on the entire path.
///   Example: `tests/*.rs` matches `.rs` files in the `tests/` folder only.
///   Example: `**/invalid.rs` matches `invalid.rs` anywhere in the directory tree.
pub fn should_ignore(path: &Path, root_path: &Path, ignore_patterns: &[String]) -> bool {
    let file_name = path.file_name().and_then(|os| os.to_str()).unwrap_or("");
    let rel_slash = to_relative_slash_path(path, root_path);

    // Configure glob matching options
    let match_opts = MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        ..Default::default()
    };

    // Always ignore .git directories and files
    if rel_slash.contains("/.git/") || rel_slash.contains("\\.git\\") || file_name == ".git" {
        return true;
    }

    // Check custom ignore patterns
    for pattern in ignore_patterns {
        if !pattern.contains('/') {
            // If the pattern has no slash, treat it as matching just the file name
            if file_name == pattern {
                return true;
            }
        } else {
            // If the pattern has a slash, treat it as a glob for the entire path
            if let Ok(gpat) = Pattern::new(pattern) {
                if gpat.matches_with(&rel_slash, match_opts) {
                    return true;
                }
            }
        }
    }
    false
}

/// Checks if a file is likely to be binary
pub fn is_likely_binary(path: &Path) -> bool {
    // Common binary file extensions
    const BINARY_EXTENSIONS: &[&str] = &[
        "exe", "dll", "so", "dylib", "bin", "obj", "o", "class", "jar", "war", "ear", "png", "jpg",
        "jpeg", "gif", "bmp", "ico", "pdf", "doc", "docx", "xls", "xlsx", "zip", "tar", "gz", "7z",
        "rar",
    ];

    if let Some(ext) = path.extension() {
        if let Some(ext_str) = ext.to_str() {
            return BINARY_EXTENSIONS
                .iter()
                .any(|&bin_ext| bin_ext.eq_ignore_ascii_case(ext_str));
        }
    }
    false
}

/// Determines if a file should be included in the search
pub fn should_include_file(
    path: &Path,
    root_path: &Path,
    extensions: &Option<Vec<String>>,
    ignore_patterns: &[String],
) -> bool {
    !is_likely_binary(path)
        && has_valid_extension(path, extensions)
        && !should_ignore(path, root_path, ignore_patterns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_valid_extension() {
        let path = Path::new("test.rs");
        let extensions = Some(vec!["rs".to_string()]);
        assert!(has_valid_extension(path, &extensions));

        let path = Path::new("test.py");
        assert!(!has_valid_extension(path, &extensions));

        let path = Path::new("test.RS"); // Test case insensitivity
        assert!(has_valid_extension(path, &extensions));

        let path = Path::new("test"); // No extension
        assert!(!has_valid_extension(path, &extensions));

        let path = Path::new("test.rs");
        let no_extensions = None;
        assert!(has_valid_extension(path, &no_extensions));
    }

    #[test]
    fn test_should_ignore() {
        let ignore_patterns = vec![
            "**/test_[0-4].txt".to_string(),
            "target/**/*.rs".to_string(),
            ".git/*".to_string(),
            "**/*.tmp".to_string(),
        ];

        // Should ignore
        assert!(should_ignore(
            Path::new("test_0.txt"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(should_ignore(
            Path::new("test_4.txt"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(should_ignore(
            Path::new("dir/test_2.txt"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(should_ignore(
            Path::new("target/debug/main.rs"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(should_ignore(
            Path::new(".git/config"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(should_ignore(
            Path::new("src/temp.tmp"),
            Path::new(""),
            &ignore_patterns
        ));

        // Should not ignore
        assert!(!should_ignore(
            Path::new("test_5.txt"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(!should_ignore(
            Path::new("test_9.txt"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(!should_ignore(
            Path::new("src/main.rs"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(!should_ignore(
            Path::new(".git2/config"),
            Path::new(""),
            &ignore_patterns
        ));
        assert!(!should_ignore(
            Path::new(".gitignore"),
            Path::new(""),
            &ignore_patterns
        ));
    }

    #[test]
    fn test_is_likely_binary() {
        assert!(is_likely_binary(Path::new("test.exe")));
        assert!(is_likely_binary(Path::new("test.dll")));
        assert!(is_likely_binary(Path::new("test.png")));
        assert!(is_likely_binary(Path::new("test.PDF"))); // Test case insensitivity
        assert!(!is_likely_binary(Path::new("test.rs")));
        assert!(!is_likely_binary(Path::new("test.txt")));
        assert!(!is_likely_binary(Path::new("test")));
    }

    #[test]
    fn test_should_include_file() {
        let extensions = Some(vec!["rs".to_string()]);
        let ignore_patterns = vec!["target/**/*.rs".to_string()];

        // Should include: .rs file, not in target, not binary
        assert!(should_include_file(
            Path::new("src/main.rs"),
            Path::new(""),
            &extensions,
            &ignore_patterns
        ));

        // Should not include: wrong extension
        assert!(!should_include_file(
            Path::new("src/main.py"),
            Path::new(""),
            &extensions,
            &ignore_patterns
        ));

        // Should not include: matches ignore pattern
        assert!(!should_include_file(
            Path::new("target/debug/main.rs"),
            Path::new(""),
            &extensions,
            &ignore_patterns
        ));

        // Should not include: binary file
        assert!(!should_include_file(
            Path::new("src/test.exe"),
            Path::new(""),
            &extensions,
            &ignore_patterns
        ));

        // Should include: .rs file in target but not matching pattern
        assert!(should_include_file(
            Path::new("target.rs"),
            Path::new(""),
            &extensions,
            &ignore_patterns
        ));
    }

    #[test]
    fn test_ignore_no_slash_pattern() {
        let p1 = Path::new("C:/Users/foo/bar/invalid.rs");
        let p2 = Path::new("C:/Users/foo/bar/other.rs");

        let patterns = vec!["invalid.rs".to_string()];
        // p1 should be ignored because its file name is "invalid.rs"
        assert!(should_ignore(p1, Path::new(""), &patterns));
        // p2 should not be ignored
        assert!(!should_ignore(p2, Path::new(""), &patterns));
    }

    #[test]
    fn test_ignore_slash_pattern() {
        let p1 = Path::new("C:/Users/foo/bar/invalid.rs");
        let patterns = vec!["**/invalid.rs".to_string()];
        // p1 matches the glob, so we ignore it
        assert!(should_ignore(p1, Path::new(""), &patterns));
    }

    #[test]
    fn test_ignore_filename_no_slash() {
        let file_1 = Path::new("C:/Users/foo/bar/invalid.rs");
        let file_2 = Path::new("C:/Users/foo/bar/other.rs");
        let file_3 = Path::new("C:/Users/foo/bar/baz/invalid.rs");

        // Pattern with no slash
        let patterns = vec!["invalid.rs".to_string()];
        // Both files named "invalid.rs" should be ignored
        assert!(should_ignore(file_1, Path::new(""), &patterns));
        assert!(should_ignore(file_3, Path::new(""), &patterns));
        // "other.rs" is not ignored
        assert!(!should_ignore(file_2, Path::new(""), &patterns));
    }

    #[test]
    fn test_ignore_with_slash_glob() {
        let root = Path::new("C:/repo");
        let file_1 = Path::new("C:/repo/tests/invalid.rs");
        let file_2 = Path::new("C:/repo/src/invalid.rs");
        let file_3 = Path::new("C:/repo/docs/README.md");

        // Pattern with slash => use full path glob
        let patterns = vec!["tests/*.rs".to_string()];
        // "tests/*.rs" should match only "tests/invalid.rs"
        assert!(should_ignore(file_1, root, &patterns));
        assert!(!should_ignore(file_2, root, &patterns));
        assert!(!should_ignore(file_3, root, &patterns));
    }

    #[test]
    fn test_ignore_star_glob_no_subdirs() {
        let root = Path::new("C:/repo");
        let file_1 = Path::new("C:/repo/src/foo.rs");
        let file_2 = Path::new("C:/repo/src/bar.rs");
        let file_3 = Path::new("C:/repo/src/nested/baz.rs");

        // Pattern "src/*.rs" matches only files directly under "src/"
        let patterns = vec!["src/*.rs".to_string()];
        assert!(should_ignore(file_1, root, &patterns));
        assert!(should_ignore(file_2, root, &patterns));
        assert!(!should_ignore(file_3, root, &patterns));
    }

    #[test]
    fn test_ignore_double_star_glob() {
        let root = Path::new("C:/repo");
        let file_1 = Path::new("C:/repo/src/foo.rs");
        let file_2 = Path::new("C:/repo/src/nested/bar.rs");

        // Pattern "src/**/*.rs" matches .rs files at ANY depth under src/
        let patterns = vec!["src/**/*.rs".to_string()];
        assert!(should_ignore(file_1, root, &patterns));
        assert!(should_ignore(file_2, root, &patterns));
    }
}
