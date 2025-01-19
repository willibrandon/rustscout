use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use rustscout::{
    errors::SearchResult,
    replace::{
        FileReplacementPlan, ReplacementConfig, ReplacementPattern, ReplacementSet, ReplacementTask,
    },
    search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode},
    workspace::init_workspace,
};

/// Helper function to create a test file with content
fn create_test_file(dir: &TempDir, name: &str, content: &str) -> SearchResult<PathBuf> {
    let path = dir.path().join(name);
    fs::write(&path, content)?;
    Ok(path)
}

#[test]
fn test_workspace_move() -> SearchResult<()> {
    // 1. Create initial workspace
    let temp = TempDir::new().unwrap();
    let initial_root = temp.path();

    // Initialize workspace
    init_workspace(initial_root, "json")?;

    // 2. Create test files and make changes
    let test_file = create_test_file(&temp, "test.txt", "original content")?;

    // Create replacement configuration
    let pattern = ReplacementPattern {
        definition: PatternDefinition {
            text: "original".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_mode: HyphenMode::default(),
        },
        replacement_text: "changed".to_string(),
    };

    let config = ReplacementConfig {
        patterns: vec![pattern],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: initial_root.join(".rustscout").join("undo"),
    };

    // Create and apply replacement
    let mut replacement_set = ReplacementSet::new(config.clone());
    let mut plan = FileReplacementPlan::new(test_file.clone())?;
    plan.add_replacement(ReplacementTask::new(
        test_file.clone(),
        (0, 8), // "original"
        "changed".to_string(),
        0,
        config.clone(),
    ))?;
    replacement_set.add_plan(plan);
    replacement_set.apply()?;

    // Verify file was changed
    let changed_content = fs::read_to_string(&test_file)?;
    assert_eq!(changed_content, "changed content");

    // Verify backup was created
    let undo_dir = initial_root.join(".rustscout").join("undo");
    assert!(undo_dir.exists(), "Undo directory should exist");
    let entries: Vec<_> = fs::read_dir(&undo_dir)?.collect();
    assert!(!entries.is_empty(), "Undo directory should not be empty");

    // 3. Move the workspace to a new location
    let new_temp = TempDir::new().unwrap();
    let new_location = new_temp.path().join("moved_workspace");
    fs::rename(initial_root, &new_location)?;

    // 4. List undo operations from new location
    let moved_config = ReplacementConfig {
        undo_dir: new_location.join(".rustscout").join("undo"),
        ..config
    };

    let undo_ops = ReplacementSet::list_undo_operations(&moved_config)?;
    assert!(
        !undo_ops.is_empty(),
        "Should find undo operations after move"
    );

    // Get the ID of the most recent undo operation
    let (undo_info, _) = undo_ops.first().unwrap();

    // 5. Attempt undo from new location
    ReplacementSet::undo_by_id(undo_info.timestamp, &moved_config)?;

    // 6. Verify file was restored
    let moved_file = new_location.join("test.txt");
    let restored_content = fs::read_to_string(&moved_file)?;
    assert_eq!(
        restored_content, "original content",
        "File should be restored to original content"
    );

    Ok(())
}

#[test]
fn test_workspace_move_multi_crate() -> SearchResult<()> {
    // 1. Create initial workspace with multiple crates
    let temp = TempDir::new().unwrap();
    let initial_root = temp.path();

    // Initialize workspace
    init_workspace(initial_root, "json")?;

    // Create crate directories
    let crate_a = initial_root.join("crate_a");
    let crate_b = initial_root.join("crate_b");
    fs::create_dir_all(&crate_a)?;
    fs::create_dir_all(&crate_b)?;

    // Create test files in each crate
    let file_a = create_test_file(&temp, "crate_a/lib.rs", "original content A")?;
    let file_b = create_test_file(&temp, "crate_b/lib.rs", "original content B")?;

    // Create replacement configuration
    let pattern = ReplacementPattern {
        definition: PatternDefinition {
            text: "original".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_mode: HyphenMode::default(),
        },
        replacement_text: "changed".to_string(),
    };

    let config = ReplacementConfig {
        patterns: vec![pattern],
        backup_enabled: true,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: true,
        undo_dir: initial_root.join(".rustscout").join("undo"),
    };

    // Create and apply replacements for both files
    let mut replacement_set = ReplacementSet::new(config.clone());

    // Add plan for crate A
    let mut plan_a = FileReplacementPlan::new(file_a.clone())?;
    plan_a.add_replacement(ReplacementTask::new(
        file_a.clone(),
        (0, 8),
        "changed".to_string(),
        0,
        config.clone(),
    ))?;
    replacement_set.add_plan(plan_a);

    // Add plan for crate B
    let mut plan_b = FileReplacementPlan::new(file_b.clone())?;
    plan_b.add_replacement(ReplacementTask::new(
        file_b.clone(),
        (0, 8),
        "changed".to_string(),
        0,
        config.clone(),
    ))?;
    replacement_set.add_plan(plan_b);

    // Apply all replacements
    replacement_set.apply()?;

    // Verify files were changed
    assert_eq!(fs::read_to_string(&file_a)?, "changed content A");
    assert_eq!(fs::read_to_string(&file_b)?, "changed content B");

    // Move the workspace to a new location
    let new_temp = TempDir::new().unwrap();
    let new_location = new_temp.path().join("moved_workspace");
    fs::rename(initial_root, &new_location)?;

    // List undo operations from new location
    let moved_config = ReplacementConfig {
        undo_dir: new_location.join(".rustscout").join("undo"),
        ..config
    };

    let undo_ops = ReplacementSet::list_undo_operations(&moved_config)?;
    assert!(
        !undo_ops.is_empty(),
        "Should find undo operations after move"
    );

    // Get the ID of the most recent undo operation
    let (undo_info, _) = undo_ops.first().unwrap();

    // Attempt undo from new location
    ReplacementSet::undo_by_id(undo_info.timestamp, &moved_config)?;

    // Verify both files were restored
    let moved_file_a = new_location.join("crate_a/lib.rs");
    let moved_file_b = new_location.join("crate_b/lib.rs");

    assert_eq!(
        fs::read_to_string(&moved_file_a)?,
        "original content A",
        "Crate A file should be restored to original content"
    );
    assert_eq!(
        fs::read_to_string(&moved_file_b)?,
        "original content B",
        "Crate B file should be restored to original content"
    );

    Ok(())
}
