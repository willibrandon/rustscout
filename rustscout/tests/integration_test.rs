use anyhow::Result;
use rustscout::search::search;
use rustscout::SearchConfig;
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::PathBuf;
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

#[test]
fn test_simple_pattern() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 10, 100)?;

    let config = SearchConfig {
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec![r"FIXME:.*bug.*line \d+".to_string()],
        pattern: String::from(r"FIXME:.*bug.*line \d+"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: Some(vec!["rs".to_string()]),
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec!["**/test_[0-4].txt".to_string()],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec![],
        pattern: String::new(),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: true,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec!["TODO".to_string(), r"FIXME:.*bug.*line \d+".to_string()],
        pattern: String::new(),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec![],
        pattern: String::new(),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
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
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 2,
        context_after: 0,
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
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 2,
        context_after: 2,
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
        patterns: vec!["TODO".to_string()],
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 1,
        context_after: 1,
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
