use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs::File;
use std::io::Write;
use tempfile::{tempdir, TempDir};

fn create_test_files(dir: &TempDir, files: &[(&str, &str)]) -> Result<()> {
    for (name, content) in files {
        let file_path = dir.path().join(name);
        let mut file = File::create(file_path)?;
        writeln!(file, "{}", content)?;
    }
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
