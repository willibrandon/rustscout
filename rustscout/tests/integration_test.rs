use rtrace_core::{search, Config};
use std::fs::{self, create_dir_all};
use std::path::Path;
use tempfile::TempDir;

// Helper function to create a test file with content
fn create_test_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

// Helper function to create a test project structure
fn create_test_project(dir: &Path) {
    // Create source files
    create_test_file(
        dir,
        "src/main.rs",
        r#"
fn main() {
    println!("Hello, world!");
    do_something();
}

fn do_something() {
    // TODO: Implement this
    println!("Not implemented");
}
"#,
    );

    create_test_file(
        dir,
        "src/lib.rs",
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    // TODO: Add error handling
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 2), 4);
    }
}
"#,
    );

    // Create test files (with a different name)
    create_test_file(
        dir,
        "tests/basic_test.rs",
        r#"
#[test]
fn test_integration() {
    // TODO: Add more tests
    assert!(true);
}
"#,
    );

    // Create documentation
    create_test_file(
        dir,
        "docs/api.md",
        r#"
# API Documentation

TODO: Document the API
"#,
    );

    // Create build artifacts (should be ignored)
    create_test_file(dir, "target/debug/main.rs", "fn main() {}");
    create_test_file(dir, "target/release/lib.rs", "// Release build");

    // Create git files (should be ignored)
    create_test_file(dir, ".git/config", "git config file");
    create_test_file(dir, ".gitignore", "target/\n*.rs.bk");
}

#[test]
fn test_find_todo_comments() {
    let temp_dir = TempDir::new().unwrap();
    create_test_project(temp_dir.path());

    let config = Config::new("TODO:".to_string(), temp_dir.path().to_path_buf())
        .with_file_extensions(vec!["rs".to_string(), "md".to_string()]);

    let result = search::search(&config).unwrap();

    // Should find TODOs in src/main.rs, src/lib.rs, tests/integration_test.rs, and docs/api.md
    assert_eq!(result.total_matches, 4, "Expected to find 4 TODO comments");
    assert_eq!(
        result.files_with_matches, 4,
        "Expected to find TODOs in 4 files"
    );

    // Verify each TODO was found
    let todos: Vec<_> = result
        .file_results
        .iter()
        .flat_map(|fr| fr.matches.iter().map(|m| m.line_content.clone()))
        .collect();

    assert!(todos.iter().any(|t| t.contains("TODO: Implement this")));
    assert!(todos.iter().any(|t| t.contains("TODO: Add error handling")));
    assert!(todos.iter().any(|t| t.contains("TODO: Add more tests")));
    assert!(todos.iter().any(|t| t.contains("TODO: Document the API")));
}

#[test]
fn test_find_rust_functions() {
    let temp_dir = TempDir::new().unwrap();
    create_test_project(temp_dir.path());

    let config = Config::new(
        r"fn\s+\w+\s*\([^)]*\)".to_string(),
        temp_dir.path().to_path_buf(),
    )
    .with_file_extensions(vec!["rs".to_string()])
    .with_ignore_patterns(vec![
        "target".to_string(), // Ignore target directory
    ]);

    let result = search::search(&config).unwrap();

    // Print all found functions and their locations for debugging
    println!("\nFound functions in files:");
    for file_result in &result.file_results {
        println!("\nIn file: {}", file_result.path.display());
        for m in &file_result.matches {
            println!("  Line {}: {}", m.line_number, m.line_content.trim());
        }
    }
    println!("\nTotal matches: {}", result.total_matches);

    // Should find: main(), do_something(), add(), test_add(), test_integration()
    assert_eq!(
        result.total_matches, 5,
        "Expected to find 5 function definitions"
    );

    // Verify each function was found
    let functions: Vec<_> = result
        .file_results
        .iter()
        .flat_map(|fr| fr.matches.iter().map(|m| m.line_content.clone()))
        .collect();

    assert!(functions.iter().any(|f| f.contains("fn main()")));
    assert!(functions.iter().any(|f| f.contains("fn do_something()")));
    assert!(functions
        .iter()
        .any(|f| f.contains("fn add(a: i32, b: i32)")));
    assert!(functions.iter().any(|f| f.contains("fn test_add()")));
    assert!(functions
        .iter()
        .any(|f| f.contains("fn test_integration()")));

    // Verify no files from target/ were searched
    for file_result in &result.file_results {
        assert!(
            !file_result.path.to_string_lossy().contains("target/"),
            "Found matches in ignored directory: {}",
            file_result.path.display()
        );
    }
}

#[test]
fn test_ignore_patterns() {
    let temp_dir = TempDir::new().unwrap();
    create_test_project(temp_dir.path());

    let config = Config::new("fn".to_string(), temp_dir.path().to_path_buf())
        .with_file_extensions(vec!["rs".to_string()])
        .with_ignore_patterns(vec!["target".to_string(), ".git".to_string()]);

    let result = search::search(&config).unwrap();

    // Verify no files from target/ or .git/ were searched
    for file_result in &result.file_results {
        let path = file_result.path.to_string_lossy();
        assert!(
            !path.contains("target/"),
            "Should not search files in target/"
        );
        assert!(!path.contains(".git/"), "Should not search files in .git/");
    }
}

#[test]
fn test_empty_and_invalid_patterns() {
    let temp_dir = TempDir::new().unwrap();
    create_test_project(temp_dir.path());

    // Test empty pattern
    let config = Config::new("".to_string(), temp_dir.path().to_path_buf());
    let result = search::search(&config).unwrap();
    assert_eq!(
        result.total_matches, 0,
        "Empty pattern should find no matches"
    );

    // Test invalid regex pattern
    let config = Config::new("[invalid".to_string(), temp_dir.path().to_path_buf());
    let result = search::search(&config);
    assert!(result.is_err(), "Invalid regex should return an error");
}

#[test]
fn test_large_file_search() {
    let temp_dir = TempDir::new().unwrap();

    // Create a large file with many matches
    let mut content = String::with_capacity(100_000);
    for i in 0..1000 {
        content.push_str(&format!("Line {} with TODO: Fix this\n", i));
    }
    create_test_file(&temp_dir.path(), "large_file.txt", &content);

    let config = Config::new("TODO: Fix this".to_string(), temp_dir.path().to_path_buf());

    let result = search::search(&config).unwrap();
    assert_eq!(
        result.total_matches, 1000,
        "Should find 1000 TODOs in large file"
    );
    assert_eq!(result.files_with_matches, 1);
}
