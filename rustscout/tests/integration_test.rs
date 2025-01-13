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
    };

    let result = search(&config)?;
    assert_eq!(result.total_matches, 0);
    assert_eq!(result.files_with_matches, 0);
    Ok(())
}
