use anyhow::Result;
use rustscout::replace::{FileReplacementPlan, ReplacementConfig, ReplacementSet, ReplacementTask};
use rustscout::search::matcher::{PatternDefinition, WordBoundaryMode, HyphenHandling};
use std::fs;
use std::path::Path;
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

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(dir.path().join("test.txt"))?;
    plan.add_replacement(ReplacementTask::new(
        dir.path().join("test.txt"),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(dir.path().join("test.txt"))?, "Hi world");
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

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: true,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(dir.path().join("test.txt"))?;
    plan.add_replacement(ReplacementTask::new(
        dir.path().join("test.txt"),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(dir.path().join("test.txt"))?, "Hi world");
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

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: true,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
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

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (13, 18),
        "Hi".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);

    let preview = replacement_set.preview()?;
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].original_lines.len(), 2);
    assert_eq!(preview[0].new_lines.len(), 2);
    assert_eq!(preview[0].original_lines[0], "Hello world!");
    assert_eq!(preview[0].new_lines[0], "Hi world!");
    assert_eq!(preview[0].original_lines[1], "Hello Rust!");
    assert_eq!(preview[0].new_lines[1], "Hi Rust!");
    Ok(())
}

#[test]
fn test_replace_undo_list() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: true,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
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
        .contains("Replace 'Hello' with 'Hi'"));
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

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: true,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config.clone());
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // Verify the file was modified
    assert_eq!(fs::read_to_string(&test_file)?, "Hi world!");

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
fn test_replace_regex() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world! Hello Rust!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let pattern_def = PatternDefinition {
        text: r"Hello\s(\w+)".to_string(),
        is_regex: true,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi $1".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 11),
        "Hi world".to_string(),
        config.clone(),
    ))?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (13, 24),
        "Hi Rust".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(&test_file)?, "Hi world! Hi Rust!");
    Ok(())
}

#[test]
fn test_replace_word_boundary() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world! HelloWorld!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::WholeWords,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(&test_file)?, "Hi world! HelloWorld!");
    Ok(())
}

#[test]
fn test_replace_hyphen_handling() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello-world! HelloWorld!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::WholeWords,
        hyphen_handling: HyphenHandling::Boundary,
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(&test_file)?, "Hi-world! HelloWorld!");
    Ok(())
}

#[test]
fn test_replace_multiple_patterns() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "Hello world! Goodbye world!")?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let pattern_def1 = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let pattern_def2 = PatternDefinition {
        text: "Goodbye".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Bye".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def1, pattern_def2],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ))?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (13, 20),
        "Bye".to_string(),
        config.clone(),
    ))?;

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(&test_file)?, "Hi world! Bye world!");
    Ok(())
}

#[test]
fn test_replace_multithreaded() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    let content = "Hello world! ".repeat(1000);
    fs::write(&test_file, &content)?;

    let undo_dir = dir.path().join(".rustscout").join("undo");
    fs::create_dir_all(&undo_dir)?;

    let pattern_def = PatternDefinition {
        text: "Hello".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::None,
        hyphen_handling: HyphenHandling::default(),
        replacement: Some("Hi".to_string()),
        capture_template: None,
    };

    let config = ReplacementConfig {
        patterns: vec![pattern_def],
        backup_enabled: false,
        dry_run: false,
        ..Default::default()
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    for i in 0..1000 {
        plan.add_replacement(ReplacementTask::new(
            test_file.clone(),
            (i * 13, i * 13 + 5),
            "Hi".to_string(),
            config.clone(),
        ))?;
    }

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    let expected_content = "Hi world! ".repeat(1000);
    assert_eq!(fs::read_to_string(&test_file)?, expected_content);
    Ok(())
}
