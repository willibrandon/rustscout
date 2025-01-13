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
use glob::Pattern;
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

/// Checks if a file should be ignored based on ignore patterns
pub fn should_ignore(path: &Path, ignore_patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();

    // Always ignore target/ and .git/ directories
    if path_str.contains("/target/") || path_str.contains("/.git/") {
        return true;
    }

    // Check custom ignore patterns
    ignore_patterns.iter().any(|pattern| {
        if let Ok(p) = Pattern::new(pattern) {
            // Convert path to a format that matches the pattern style
            let normalized_path = path_str.replace('\\', "/");
            p.matches(&normalized_path)
        } else {
            false
        }
    })
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
    extensions: &Option<Vec<String>>,
    ignore_patterns: &[String],
) -> bool {
    !is_likely_binary(path)
        && has_valid_extension(path, extensions)
        && !should_ignore(path, ignore_patterns)
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
        assert!(should_ignore(Path::new("test_0.txt"), &ignore_patterns));
        assert!(should_ignore(Path::new("test_4.txt"), &ignore_patterns));
        assert!(should_ignore(Path::new("dir/test_2.txt"), &ignore_patterns));
        assert!(should_ignore(
            Path::new("target/debug/main.rs"),
            &ignore_patterns
        ));
        assert!(should_ignore(Path::new(".git/config"), &ignore_patterns));
        assert!(should_ignore(Path::new("src/temp.tmp"), &ignore_patterns));

        // Should not ignore
        assert!(!should_ignore(Path::new("test_5.txt"), &ignore_patterns));
        assert!(!should_ignore(Path::new("test_9.txt"), &ignore_patterns));
        assert!(!should_ignore(Path::new("src/main.rs"), &ignore_patterns));
        assert!(!should_ignore(Path::new(".git2/config"), &ignore_patterns));
        assert!(!should_ignore(Path::new(".gitignore"), &ignore_patterns));
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
            &extensions,
            &ignore_patterns
        ));

        // Should not include: wrong extension
        assert!(!should_include_file(
            Path::new("src/main.py"),
            &extensions,
            &ignore_patterns
        ));

        // Should not include: matches ignore pattern
        assert!(!should_include_file(
            Path::new("target/debug/main.rs"),
            &extensions,
            &ignore_patterns
        ));

        // Should not include: binary file
        assert!(!should_include_file(
            Path::new("src/test.exe"),
            &extensions,
            &ignore_patterns
        ));

        // Should include: .rs file in target but not matching pattern
        assert!(should_include_file(
            Path::new("target.rs"),
            &extensions,
            &ignore_patterns
        ));
    }
}
