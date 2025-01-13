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
        Pattern::new(pattern)
            .map(|p| p.matches(&path_str))
            .unwrap_or(false)
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
            "target/**/*.rs".to_string(), // All Rust files under target
            ".git/*".to_string(),         // Direct children of .git
            "**/*.tmp".to_string(),       // Any tmp files
        ];

        // Should ignore
        assert!(should_ignore(
            Path::new("target/debug/main.rs"),
            &ignore_patterns
        ));
        assert!(should_ignore(
            Path::new("target/release/lib.rs"),
            &ignore_patterns
        ));
        assert!(should_ignore(Path::new(".git/config"), &ignore_patterns));
        assert!(should_ignore(Path::new("src/temp.tmp"), &ignore_patterns));
        assert!(should_ignore(
            Path::new("deep/path/file.tmp"),
            &ignore_patterns
        ));

        // Should not ignore
        assert!(!should_ignore(Path::new("src/main.rs"), &ignore_patterns));
        assert!(!should_ignore(Path::new(".git2/config"), &ignore_patterns));
        assert!(!should_ignore(
            Path::new("target/debug/main.txt"),
            &ignore_patterns
        ));
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
