use anyhow::Result;
use assert_cmd::prelude::*;
use predicates::prelude::*;
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    replace::{
        FileReplacementPlan, ReplacementConfig, ReplacementPattern, ReplacementSet, ReplacementTask,
    },
    search,
    search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode},
};
use std::process::Command;
use std::{fs, num::NonZeroUsize, path::Path};
use tempfile::tempdir;

// Helper function to create test files
fn create_test_files(dir: impl AsRef<Path>, files: &[(&str, &str)]) -> Result<()> {
    for (name, content) in files {
        fs::write(dir.as_ref().join(name), content)?;
    }
    Ok(())
}

#[test]
fn test_replace_basic() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, &[("test.txt", "Hello world")])?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: "Hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::Joining,
            },
            replacement_text: "World".to_string(),
        }],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(dir.path().join("test.txt"))?;
    plan.add_replacement(ReplacementTask::new(
        dir.path().join("test.txt"),
        (0, 5),
        "World".to_string(),
        0,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(
        fs::read_to_string(dir.path().join("test.txt"))?,
        "World world"
    );
    Ok(())
}

#[test]
fn test_replace_with_backup() -> Result<()> {
    let dir = tempdir()?;
    let backup_dir = dir.path().join("backups");
    fs::create_dir_all(&backup_dir)?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    create_test_files(&dir, &[("test.txt", "Hello world")])?;

    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: "Hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::Joining,
            },
            replacement_text: "World".to_string(),
        }],
        backup_enabled: true,
        dry_run: false,
        backup_dir: Some(backup_dir.clone()),
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(dir.path().join("test.txt"))?;
    plan.add_replacement(ReplacementTask::new(
        dir.path().join("test.txt"),
        (0, 5),
        "World".to_string(),
        0,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(
        fs::read_to_string(dir.path().join("test.txt"))?,
        "World world"
    );
    assert!(fs::read_dir(&backup_dir)?.filter_map(|e| e.ok()).any(|e| e
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("test.txt"))
        .unwrap_or(false)));
    Ok(())
}

#[test]
fn test_replace_dry_run() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    let original_content = "Hello world!";
    fs::write(&test_file, original_content)?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: "Hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::Joining,
            },
            replacement_text: "World".to_string(),
        }],
        backup_enabled: true,
        dry_run: true,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "World".to_string(),
        0,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // File should remain unchanged in dry-run mode
    assert_eq!(fs::read_to_string(&test_file)?, original_content);
    Ok(())
}

#[test]
fn test_replace_preview() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world!\nHello Rust!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: "Hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::Joining,
            },
            replacement_text: "World".to_string(),
        }],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "World".to_string(),
        0,
        config.clone(),
    ))?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (13, 18),
        "World".to_string(),
        0,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);

    let preview = replacement_set.preview()?;
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].original_lines.len(), 2);
    assert_eq!(preview[0].new_lines.len(), 2);
    assert_eq!(preview[0].original_lines[0], "Hello world!");
    assert_eq!(preview[0].new_lines[0], "World world!");
    assert_eq!(preview[0].original_lines[1], "Hello Rust!");
    assert_eq!(preview[0].new_lines[1], "World Rust!");
    Ok(())
}

#[test]
fn test_replace_undo_list() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: "Hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::Joining,
            },
            replacement_text: "World".to_string(),
        }],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "World".to_string(),
        0,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config.clone());
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // Check that undo information was saved
    let operations = ReplacementSet::list_undo_operations(&config)?;
    assert_eq!(operations.len(), 1);
    assert!(operations[0]
        .0
        .description
        .contains("Replace 'Hello' with 'World'"));
    Ok(())
}

#[test]
fn test_replace_undo_restore() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    let original_content = "Hello world!";
    fs::write(&test_file, original_content)?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: "Hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_mode: HyphenMode::Joining,
            },
            replacement_text: "World".to_string(),
        }],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "World".to_string(),
        0,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config.clone());
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // Verify the file was modified
    assert_eq!(fs::read_to_string(&test_file)?, "World world!");

    // Get the undo operation ID
    let operations = ReplacementSet::list_undo_operations(&config)?;
    assert_eq!(operations.len(), 1);
    let undo_id = operations[0].0.timestamp;

    // Undo the changes
    ReplacementSet::undo_by_id(undo_id, &config)?;

    // Verify the file was restored
    assert_eq!(fs::read_to_string(&test_file)?, original_content);
    Ok(())
}

#[test]
fn test_replace_cli_args() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "foo test foo-bar")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    // Simulate CLI args: replace -p "foo" -r "bar" --regex -w --hyphen-handling boundary --dry-run
    let config = ReplacementConfig {
        patterns: vec![ReplacementPattern {
            definition: PatternDefinition {
                text: r"\bfoo\b".to_string(),
                is_regex: true,
                boundary_mode: WordBoundaryMode::WholeWords,
                hyphen_mode: HyphenMode::Boundary,
            },
            replacement_text: "bar".to_string(),
        }],
        backup_enabled: true,
        dry_run: true,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    // Create search config to find matches
    let search_config = SearchConfig {
        pattern_definitions: config
            .patterns
            .iter()
            .map(|p| p.definition.clone())
            .collect(),
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

    // Find matches
    let search_result = search(&search_config)?;

    // Create replacement set
    let mut replacement_set = ReplacementSet::new(config.clone());

    // Create plans for each file with matches
    for file_result in &search_result.file_results {
        let mut plan = FileReplacementPlan::new(file_result.path.clone())?;
        for m in &file_result.matches {
            plan.add_replacement(ReplacementTask::new(
                file_result.path.clone(),
                (m.start, m.end),
                config.patterns[0].replacement_text.clone(),
                0,
                config.clone(),
            ))?;
        }
        replacement_set.add_plan(plan);
    }

    // Apply in dry-run mode
    replacement_set.apply()?;

    // Verify file wasn't changed (dry run)
    assert_eq!(fs::read_to_string(&test_file)?, "foo test foo-bar");

    // Verify preview shows correct changes
    let preview = replacement_set.preview()?;
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].original_lines[0], "foo test foo-bar");
    assert_eq!(preview[0].new_lines[0], "bar test bar-bar");

    Ok(())
}

#[test]
fn test_replace_multiple_patterns() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world! Goodbye world!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let config = ReplacementConfig {
        patterns: vec![
            ReplacementPattern {
                definition: PatternDefinition {
                    text: "Hello".to_string(),
                    is_regex: false,
                    boundary_mode: WordBoundaryMode::None,
                    hyphen_mode: HyphenMode::Joining,
                },
                replacement_text: "Hi".to_string(),
            },
            ReplacementPattern {
                definition: PatternDefinition {
                    text: "Goodbye".to_string(),
                    is_regex: false,
                    boundary_mode: WordBoundaryMode::None,
                    hyphen_mode: HyphenMode::Joining,
                },
                replacement_text: "Bye".to_string(),
            },
        ],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: undo_dir.clone(),
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    // Add first replacement
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        0,
        config.clone(),
    ))?;

    // Add second replacement
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (13, 20),
        "Bye".to_string(),
        1,
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(&test_file)?, "Hi world! Bye world!");
    Ok(())
}

#[test]
fn test_search_hyphen_mode() -> Result<()> {
    let dir = tempdir()?;
    create_test_files(
        &dir,
        &[
            ("code.txt", "test-case\ntest case\npretest-case"),
            ("text.txt", "hello-world\ngood-morning\nworld-class"),
        ],
    )?;

    // Test joining mode (default, for code identifiers)
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "test".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::WholeWords,
            hyphen_mode: HyphenMode::Joining, // --hyphen-mode=joining
        }],
        root_path: dir.path().to_path_buf(),
        ..SearchConfig::default()
    };

    let results = search(&config)?;
    assert_eq!(results.total_matches, 1); // Should only match "test case"
    assert_eq!(results.files_with_matches, 1);
    let file_result = &results.file_results[0];
    assert!(file_result
        .matches
        .iter()
        .any(|m| m.line_content.contains("test case")));

    // Test boundary mode (for natural text)
    let config = SearchConfig {
        pattern_definitions: vec![PatternDefinition {
            text: "hello".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::WholeWords,
            hyphen_mode: HyphenMode::Boundary, // --hyphen-mode=boundary
        }],
        root_path: dir.path().to_path_buf(),
        ..SearchConfig::default()
    };

    let results = search(&config)?;
    assert_eq!(results.total_matches, 1);
    assert_eq!(results.files_with_matches, 1);
    let file_result = &results.file_results[0];
    assert!(file_result
        .matches
        .iter()
        .any(|m| m.line_content.contains("hello-world")));

    Ok(())
}

#[test]
fn test_interactive_search() -> Result<()> {
    let temp_dir = tempdir()?;

    // Create test files
    create_test_files(
        &temp_dir,
        &[
            ("file1.txt", "Hello world\nTODO: Fix this\nGoodbye"),
            ("file2.txt", "Another TODO here\nSome text"),
        ],
    )?;

    // Set up environment for test mode
    std::env::set_var("INTERACTIVE_TEST", "1");

    let mut cmd = Command::cargo_bin("rustscout-cli")?;
    cmd.args([
        "interactive-search",
        "-p",
        "TODO",
        "-d",
        temp_dir.path().to_str().unwrap(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Found 2 matches"))
        .stdout(predicate::str::contains("TODO: Fix this"))
        .stdout(predicate::str::contains("Another TODO here"))
        .stdout(predicate::str::contains("Match 1 of 2"))
        .stdout(predicate::str::contains("Match 2 of 2"));

    // Clean up test environment
    std::env::remove_var("INTERACTIVE_TEST");
    Ok(())
}
