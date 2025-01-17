use anyhow::Result;
use rustscout::search::search;
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    errors::unify_path,
    search::matcher::{HyphenHandling, PatternDefinition, WordBoundaryMode},
    SearchError,
};
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn create_test_files(
    dir: &tempfile::TempDir,
    file_count: usize,
    lines_per_file: usize,
) -> Result<()> {
    for i in 0..file_count {
        let file_path = dir.path().join(format!("test_{}.txt", i));
        let mut file = File::create(file_path)?;
        for j in 0..lines_per_file {
            writeln!(file, "Line {} in file {}: TODO implement this", j, i)?;
            writeln!(file, "Another line {} in file {}: nothing special", j, i)?;
            writeln!(file, "FIXME: This is a bug in file {} line {}", i, j)?;
        }
    }
    Ok(())
}

/// Helper function to create a file with specific content and encoding
fn create_test_file(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> Result<PathBuf> {
    let path = dir.path().join(name);
    std::fs::write(&path, content)?;
    Ok(path)
}

// Convert a `Path` into a vector of wide characters (u16).
#[cfg(target_os = "windows")]
fn path_wide_chars(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().collect()
}

#[cfg(not(target_os = "windows"))]
fn path_wide_chars(_p: &Path) -> Vec<u16> {
    Vec::new() // Return empty vector on non-Windows platforms
}

#[test]
fn test_simple_pattern() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(4).unwrap(),
        log_level: "info".to_string(),
        context_before: 1,
        context_after: 1,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::Auto,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert!(result.total_matches > 0);
    assert!(result.files_with_matches > 0);
    Ok(())
}

#[test]
fn test_regex_pattern() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: r"FIXME:.*bug.*line \d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
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
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert!(result.total_matches > 0);
    assert!(result.files_with_matches > 0);
    Ok(())
}

#[test]
fn test_file_extensions() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    // Create a .rs file
    let rs_file = dir.path().join("test.rs");
    let mut file = File::create(rs_file)?;
    writeln!(file, "// TODO: Implement this function")?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: Some(vec!["rs".to_string()]),
        ignore_patterns: vec![],
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
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert_eq!(result.files_with_matches, 1);
    assert_eq!(result.total_matches, 1);
    Ok(())
}

#[test]
fn test_ignore_patterns() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec!["**/test_[0-4].txt".to_string()],
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
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert!(
        result.files_with_matches <= 5,
        "Expected at most 5 files with matches, got {}",
        result.files_with_matches
    );
    Ok(())
}

#[test]
fn test_empty_pattern() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 1, 10)?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: String::new(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
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
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 0);
    assert_eq!(result.files_with_matches, 0);
    Ok(())
}

#[test]
fn test_stats_only() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: true,
        thread_count: NonZeroUsize::new(4).unwrap(),
        log_level: "info".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::Auto,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert!(result.total_matches > 0);
    assert!(result.files_with_matches > 0);
    Ok(())
}

#[test]
fn test_multiple_patterns() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    let config = SearchConfig {
        pattern_definitions: vec![
            PatternDefinition {
                text: "TODO".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_handling: HyphenHandling::default(),
            },
            PatternDefinition {
                text: "FIXME.*bug".to_string(),
                is_regex: true,
                boundary_mode: WordBoundaryMode::None,
                hyphen_handling: HyphenHandling::default(),
            },
        ],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
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
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert!(result.total_matches > 0);
    assert!(result.files_with_matches > 0);

    // Verify we find both pattern types
    let mut found_todo = false;
    let mut found_fixme = false;

    for file_result in &result.file_results {
        for m in &file_result.matches {
            if m.line_content.contains("TODO") {
                found_todo = true;
            }
            if m.line_content.contains("FIXME") && m.line_content.contains("bug") {
                found_fixme = true;
            }
        }
    }

    assert!(found_todo, "Should find TODO patterns");
    assert!(found_fixme, "Should find FIXME patterns");
    Ok(())
}

#[test]
fn test_empty_patterns() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 1, 10)?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: String::new(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
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
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 0);
    assert_eq!(result.files_with_matches, 0);
    Ok(())
}

#[test]
fn test_context_lines() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path)?;

    // Create a file with known content for testing context lines
    writeln!(file, "Line 1: Some content")?;
    writeln!(file, "Line 2: More content")?;
    writeln!(file, "Line 3: TODO: Fix this")?;
    writeln!(file, "Line 4: Implementation")?;
    writeln!(file, "Line 5: More code")?;

    // Test context before
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(4).unwrap(),
        log_level: "info".to_string(),
        context_before: 2,
        context_after: 0,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::Auto,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 1);
    let m = &result.file_results[0].matches[0];
    assert_eq!(m.context_before.len(), 2);
    assert_eq!(m.context_before[0].0, 1);
    assert_eq!(m.context_before[1].0, 2);
    assert!(m.context_after.is_empty());

    // Test context after
    let config = SearchConfig {
        context_before: 0,
        context_after: 2,
        ..config
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 1);
    let m = &result.file_results[0].matches[0];
    assert!(m.context_before.is_empty());
    assert_eq!(m.context_after.len(), 2);
    assert_eq!(m.context_after[0].0, 4);
    assert_eq!(m.context_after[1].0, 5);

    // Test both context before and after
    let config = SearchConfig {
        context_before: 1,
        context_after: 1,
        ..config
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 1);
    let m = &result.file_results[0].matches[0];
    assert_eq!(m.context_before.len(), 1);
    assert_eq!(m.context_before[0].0, 2);
    assert_eq!(m.context_after.len(), 1);
    assert_eq!(m.context_after[0].0, 4);

    Ok(())
}

#[test]
fn test_context_lines_at_file_boundaries() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path)?;

    // Create a file with matches at the start and end
    writeln!(file, "TODO: First line")?;
    writeln!(file, "Some content")?;
    writeln!(file, "More content")?;
    writeln!(file, "TODO: Last line")?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(4).unwrap(),
        log_level: "info".to_string(),
        context_before: 2,
        context_after: 2,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::Auto,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);

    // Check first match (at start of file)
    let first_match = &result.file_results[0].matches[0];
    assert!(first_match.context_before.is_empty()); // No lines before
    assert_eq!(first_match.context_after.len(), 2); // Two lines after

    // Check last match (at end of file)
    let last_match = &result.file_results[0].matches[1];
    assert_eq!(last_match.context_before.len(), 2); // Two lines before
    assert!(last_match.context_after.is_empty()); // No lines after

    Ok(())
}

#[test]
fn test_overlapping_context() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path)?;

    // Create a file with closely spaced matches
    writeln!(file, "Line 1")?;
    writeln!(file, "TODO: First")?;
    writeln!(file, "Line 3")?;
    writeln!(file, "TODO: Second")?;
    writeln!(file, "Line 5")?;

    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "TODO".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(4).unwrap(),
        log_level: "info".to_string(),
        context_before: 1,
        context_after: 1,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::Auto,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);

    // Check first match
    let first_match = &result.file_results[0].matches[0];
    assert_eq!(first_match.context_before.len(), 1);
    assert_eq!(first_match.context_after.len(), 1);

    // Check second match
    let second_match = &result.file_results[0].matches[1];
    assert_eq!(second_match.context_before.len(), 1);
    assert_eq!(second_match.context_after.len(), 1);

    Ok(())
}

#[test]
fn test_incremental_search_with_compression() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

    let cache_path = dir.path().join("cache.json");
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "pattern_\\d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![
            ".git".to_string(),
            ".git/*".to_string(),
            "**/.git/**".to_string(),
        ],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: true,
        cache_path: Some(cache_path.clone()),
        cache_strategy: ChangeDetectionStrategy::FileSignature,
        max_cache_size: Some(1024 * 1024), // 1MB
        use_compression: true,
        encoding_mode: EncodingMode::FailFast,
    };

    // First search should create compressed cache
    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);
    assert!(cache_path.exists());

    // Second search should use compressed cache
    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);

    Ok(())
}

#[test]
fn test_incremental_search_with_renames() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

    let cache_path = dir.path().join("cache.json");
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "pattern_\\d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![
            ".git".to_string(),
            ".git/*".to_string(),
            "**/.git/**".to_string(),
        ],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: true,
        cache_path: Some(cache_path.clone()),
        cache_strategy: ChangeDetectionStrategy::FileSignature,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    // First search should create cache
    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);

    // Rename file
    let new_path = dir.path().join("test_renamed.txt");
    std::fs::rename(&file_path, &new_path)?;

    // Search should handle renamed file
    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);

    Ok(())
}

#[test]
fn test_incremental_search_cache_invalidation() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

    let cache_path = dir.path().join("cache.json");
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "pattern_\\d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![
            ".git".to_string(),
            ".git/*".to_string(),
            "**/.git/**".to_string(),
        ],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: true,
        cache_path: Some(cache_path.clone()),
        cache_strategy: ChangeDetectionStrategy::FileSignature,
        max_cache_size: Some(1024), // Very small cache
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    // First search should create cache
    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);

    // Add more files to exceed cache size
    for i in 0..10 {
        let path = dir.path().join(format!("test_{}.txt", i));
        std::fs::write(&path, "pattern_1\npattern_2\n")?;
    }

    // Search should handle cache invalidation
    let result = search(&config)?;
    assert_eq!(result.total_matches, 22); // 11 files * 2 matches

    Ok(())
}

#[test]
fn test_incremental_search_git_strategy() -> Result<()> {
    let dir = tempdir()?;

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()?;

    // Create and add initial file
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

    std::process::Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(dir.path())
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()?;

    let cache_path = dir.path().join("cache.json");
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "pattern_\\d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        // Add comprehensive .git ignore patterns
        ignore_patterns: vec![
            ".git".to_string(),
            ".git/*".to_string(),
            "**/.git/**".to_string(),
        ],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: true,
        cache_path: Some(cache_path.clone()),
        cache_strategy: ChangeDetectionStrategy::GitStatus,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    // First search should create cache
    let result = search(&config)?;
    assert_eq!(result.total_matches, 2);
    assert!(cache_path.exists());

    // Modify file without git add
    std::fs::write(&file_path, "pattern_1\npattern_2\npattern_3\n")?;

    // Second search should detect the change via git status
    let result = search(&config)?;
    assert_eq!(result.total_matches, 3);

    // Add and commit the change
    std::process::Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Update file"])
        .current_dir(dir.path())
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()?;

    // Third search should use cache since file is committed
    let result = search(&config)?;
    assert_eq!(result.total_matches, 3);

    Ok(())
}

#[test]
fn test_incremental_search_corrupt_cache() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

    let cache_path = dir.path().join("cache.json");
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "pattern_\\d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![
            ".git".to_string(),
            ".git/*".to_string(),
            "**/.git/**".to_string(),
        ],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(4).unwrap(),
        log_level: "info".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: true,
        cache_path: Some(cache_path.clone()),
        cache_strategy: ChangeDetectionStrategy::FileSignature,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    // First search should create cache
    let result = search(&config)?;
    assert_eq!(result.files_with_matches, 1);
    assert_eq!(result.total_matches, 2);
    assert!(cache_path.exists());

    // Corrupt the cache file
    std::fs::write(&cache_path, "invalid json content")?;

    // Search should handle corrupt cache gracefully
    let result = search(&config)?;
    assert_eq!(result.files_with_matches, 1);
    assert_eq!(result.total_matches, 2);
    assert!(cache_path.exists());

    // Cache should be regenerated
    let result = search(&config)?;
    assert_eq!(result.files_with_matches, 1);
    assert_eq!(result.total_matches, 2);

    Ok(())
}

#[test]
fn test_incremental_search_concurrent_mods() -> Result<()> {
    let dir = tempdir()?;
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

    let cache_path = dir.path().join("cache.json");
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "pattern_\\d+".to_string(),
            is_regex: true,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }],
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![
            ".git".to_string(),
            ".git/*".to_string(),
            "**/.git/**".to_string(),
        ],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: true,
        cache_path: Some(cache_path.clone()),
        cache_strategy: ChangeDetectionStrategy::FileSignature,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    };

    // Start search in a separate thread
    let config_clone = config.clone();
    let _path_clone = file_path.clone();
    let handle = std::thread::spawn(move || {
        let result = search(&config_clone);
        // Sleep to simulate longer processing
        std::thread::sleep(std::time::Duration::from_millis(100));
        result
    });

    // Modify file while search is running
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(&file_path, "pattern_1\npattern_2\npattern_3\n")?;

    // Wait for search to complete
    let result = handle.join().unwrap()?;

    // Results should be consistent with either the old or new file state
    assert!(
        result.total_matches == 2 || result.total_matches == 3,
        "Expected 2 or 3 matches, got {}",
        result.total_matches
    );

    // Second search should see the new content
    let result = search(&config)?;
    assert_eq!(result.total_matches, 3);

    Ok(())
}

#[test]
fn test_utf8_handling_fail_fast() -> Result<()> {
    let dir = tempfile::TempDir::new()?;

    // Valid ASCII file
    let _ascii_path = create_test_file(&dir, "hello.rs", b"Hello, world!\nTODO: test this")?;

    // Valid UTF-8 with accents
    let _accented_path = create_test_file(
        &dir,
        "spanish.rs",
        "// función de prueba\nfn test_función() {\n    // TODO: implementar\n}".as_bytes(),
    )?;

    // Invalid UTF-8 file
    let invalid_path = create_test_file(
        &dir,
        "invalid.rs",
        &[
            b"fn test() {\n    println!(\"Hello\");"[..].to_vec(),
            vec![0xFF, 0xFF], // Invalid UTF-8 bytes
            b"\n}".to_vec(),
        ]
        .concat(),
    )?;

    // Create config with fail-fast mode
    let mut config =
        SearchConfig::new_with_pattern("TODO".to_string(), false, WordBoundaryMode::None);
    config.root_path = dir.path().to_path_buf();
    config.encoding_mode = EncodingMode::FailFast;

    // Search and verify results
    let result = search(&config);
    assert!(result.is_err(), "Expected error for invalid UTF-8 file");

    if let Err(e) = result {
        match e {
            SearchError::EncodingError { path, .. } => {
                // Apply unify_path to both sides for consistent comparison
                let expected = unify_path(&invalid_path);

                eprintln!("=== TEST DEBUG ===");
                eprintln!("Got error path: {:?}", path);
                eprintln!("Expected path:  {:?}", expected);

                let got_wide = path_wide_chars(&path);
                let exp_wide = path_wide_chars(&expected);
                eprintln!("Got wide chars: {:?}", got_wide);
                eprintln!("Exp wide chars: {:?}", exp_wide);

                // Compare path components
                let got_components: Vec<_> = path.components().collect();
                let exp_components: Vec<_> = expected.components().collect();
                eprintln!("Got components: {:?}", got_components);
                eprintln!("Exp components: {:?}", exp_components);

                // Also compare to_string_lossy
                let got_lossy = path.to_string_lossy();
                let exp_lossy = expected.to_string_lossy();
                eprintln!("Got lossy: {:?}", got_lossy);
                eprintln!("Exp lossy: {:?}", exp_lossy);

                eprintln!("String eq check: {}", got_lossy == exp_lossy);
                eprintln!("===============\n");

                assert_eq!(
                    path, expected,
                    "Error should reference invalid file. Got: {:?}, Expected: {:?}",
                    path, expected
                );
            }
            _ => panic!("Expected EncodingError, got: {:?}", e),
        }
    }

    // Now search only valid files
    config.ignore_patterns = vec!["invalid.rs".to_string()];
    let result = search(&config)?;

    eprintln!("\n=== SEARCH RESULTS DEBUG ===");
    eprintln!("Total matches found: {}", result.total_matches);
    eprintln!("Files with matches: {}", result.files_with_matches);
    eprintln!("Expected: 2 matches in 2 files");
    eprintln!("=========================\n");

    assert_eq!(
        result.total_matches, 2,
        "Expected matches in both valid files"
    );
    assert_eq!(
        result.files_with_matches, 2,
        "Expected matches in both valid files"
    );

    Ok(())
}

#[test]
fn test_utf8_handling_lossy() -> Result<()> {
    let dir = tempdir()?;

    let _mixed_path = create_test_file(
        &dir,
        "mixed.rs",
        &[
            b"// TODO: first task\n"[..].to_vec(),
            vec![0xFF, 0xFF], // Invalid UTF-8
            b"\n// TODO: second task".to_vec(),
        ]
        .concat(),
    )?;

    // Create config with lossy mode
    let mut config =
        SearchConfig::new_with_pattern("TODO".to_string(), false, WordBoundaryMode::None);
    config.root_path = dir.path().to_path_buf();
    config.encoding_mode = EncodingMode::Lossy;

    // Search and verify results
    let result = search(&config)?;

    assert_eq!(result.total_matches, 2, "Expected both TODOs to be found");
    assert_eq!(
        result.files_with_matches, 1,
        "Expected matches in the mixed file"
    );

    Ok(())
}

#[test]
fn test_utf8_large_file_performance() -> Result<()> {
    let dir = tempfile::TempDir::new()?;

    // Create a large file (>10MB) with valid UTF-8
    let mut large_content = String::with_capacity(11 * 1024 * 1024);
    for i in 0..100_000 {
        if i % 1000 == 0 {
            large_content.push_str("// TODO: test this\n");
        }
        large_content.push_str("// Some normal comment with unicode: función μ π\n");
    }

    let _large_path = create_test_file(&dir, "large.rs", large_content.as_bytes())?;

    // Test both modes for performance
    for mode in [EncodingMode::FailFast, EncodingMode::Lossy] {
        let mut config =
            SearchConfig::new_with_pattern("TODO".to_string(), false, WordBoundaryMode::None);
        config.root_path = dir.path().to_path_buf();
        config.encoding_mode = mode;

        let start = std::time::Instant::now();
        let result = search(&config)?;
        let duration = start.elapsed();

        assert_eq!(result.total_matches, 100, "Expected 100 TODOs");
        assert!(
            duration.as_secs_f64() < 6.0,
            "Search took too long: {:?}",
            duration
        );
    }

    Ok(())
}

#[test]
fn test_utf8_mixed_files() -> Result<()> {
    let dir = tempfile::TempDir::new()?;

    // Create multiple files with different encodings
    let _valid1_path = create_test_file(&dir, "valid1.rs", b"// TODO: test this")?;
    let _valid2_path = create_test_file(&dir, "valid2.rs", "// TODO: test función".as_bytes())?;
    let _invalid_path = create_test_file(
        &dir,
        "invalid.rs",
        &[b"// TODO: "[..].to_vec(), vec![0xFF], b" test".to_vec()].concat(),
    )?;

    // Test fail-fast mode
    {
        let mut config =
            SearchConfig::new_with_pattern("TODO".to_string(), false, WordBoundaryMode::None);
        config.root_path = dir.path().to_path_buf();
        config.encoding_mode = EncodingMode::FailFast;

        let result = search(&config);
        assert!(result.is_err(), "Expected error in fail-fast mode");
    }

    // Test lossy mode
    {
        let mut config =
            SearchConfig::new_with_pattern("TODO".to_string(), false, WordBoundaryMode::None);
        config.root_path = dir.path().to_path_buf();
        config.encoding_mode = EncodingMode::Lossy;

        let result = search(&config)?;
        assert_eq!(result.total_matches, 3, "Expected matches in all files");
        assert_eq!(
            result.files_with_matches, 3,
            "Expected all files to have matches"
        );
    }

    Ok(())
}
