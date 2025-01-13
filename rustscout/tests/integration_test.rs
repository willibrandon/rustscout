use anyhow::Result;
use rustscout::search::search;
use rustscout::Config;
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

    let config = Config {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

    let config = Config {
        pattern: String::from(r"FIXME:.*bug.*line \d+"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

    let config = Config {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: Some(vec!["rs".to_string()]),
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

    let config = Config {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec!["**/test_[0-4].txt".to_string()],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
    };

    let result = search(&config)?;
    assert!(result.files_with_matches <= 5, 
        "Expected at most 5 files with matches, got {}", result.files_with_matches);
    Ok(())
}

#[test]
fn test_empty_pattern() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 1, 10)?;

    let config = Config {
        pattern: String::new(),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

    let config = Config {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: true,
        thread_count: NonZeroUsize::new(1).unwrap(),
    };

    let result = search(&config)?;
    assert!(result.total_matches > 0);
    assert!(result.files_with_matches > 0);
    Ok(())
}
