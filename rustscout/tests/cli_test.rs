use anyhow::Result;
use rustscout::replace::{
    set_undo_dir, FileReplacementPlan, ReplacementConfig, ReplacementSet, ReplacementTask,
};
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

    let config = ReplacementConfig {
        pattern: "Hello".to_string(),
        replacement: "Hi".to_string(),
        is_regex: false,
        backup_enabled: false,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: false,
        capture_groups: None,
    };

    let mut plan = FileReplacementPlan::new(dir.path().join("test.txt"))?;
    plan.add_replacement(ReplacementTask::new(
        dir.path().join("test.txt"),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ));

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

    create_test_files(&dir, &[("test.txt", "Hello world")])?;

    let config = ReplacementConfig {
        pattern: "Hello".to_string(),
        replacement: "Hi".to_string(),
        is_regex: false,
        backup_enabled: true,
        dry_run: false,
        backup_dir: Some(backup_dir.clone()),
        preserve_metadata: false,
        capture_groups: None,
    };

    let mut plan = FileReplacementPlan::new(dir.path().join("test.txt"))?;
    plan.add_replacement(ReplacementTask::new(
        dir.path().join("test.txt"),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ));

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    assert_eq!(fs::read_to_string(dir.path().join("test.txt"))?, "Hi world");
    assert!(backup_dir.join("test.txt").exists());
    Ok(())
}

#[test]
fn test_replace_dry_run() -> Result<()> {
    let dir = tempdir()?;
    let test_file = dir.path().join("test.txt");
    let original_content = "Hello world!";
    fs::write(&test_file, original_content)?;

    let config = ReplacementConfig {
        pattern: "Hello".to_string(),
        replacement: "Hi".to_string(),
        is_regex: false,
        backup_enabled: false,
        dry_run: true,
        backup_dir: None,
        preserve_metadata: false,
        capture_groups: None,
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ));

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

    let config = ReplacementConfig {
        pattern: "Hello".to_string(),
        replacement: "Hi".to_string(),
        is_regex: false,
        backup_enabled: false,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: false,
        capture_groups: None,
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ));
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (13, 18),
        "Hi".to_string(),
        config.clone(),
    ));

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

    // Set up a temporary undo directory
    let undo_dir = dir.path().join(".rustscout").join("undo");
    set_undo_dir(undo_dir.to_str().unwrap());

    let config = ReplacementConfig {
        pattern: "Hello".to_string(),
        replacement: "Hi".to_string(),
        is_regex: false,
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: false,
        capture_groups: None,
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ));

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // Check that undo information was saved
    let operations = ReplacementSet::list_undo_operations()?;
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

    // Set up a temporary undo directory
    let undo_dir = dir.path().join(".rustscout").join("undo");
    set_undo_dir(undo_dir.to_str().unwrap());

    let config = ReplacementConfig {
        pattern: "Hello".to_string(),
        replacement: "Hi".to_string(),
        is_regex: false,
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: false,
        capture_groups: None,
    };

    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 5),
        "Hi".to_string(),
        config.clone(),
    ));

    let mut replacement_set = ReplacementSet::new(config);
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // Verify the file was modified
    assert_eq!(fs::read_to_string(&test_file)?, "Hi world!");

    // Undo the changes
    ReplacementSet::undo_by_id(0)?;

    // Verify the file was restored
    assert_eq!(fs::read_to_string(&test_file)?, original_content);
    Ok(())
}
