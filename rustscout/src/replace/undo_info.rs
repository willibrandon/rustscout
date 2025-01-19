use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::errors::{SearchError, SearchResult};
use crate::workspace::detect_workspace_root;

/// A reference to a file that can be stored with both absolute and relative paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoFileReference {
    /// Path relative to the workspace root
    pub rel_path: PathBuf,
    /// Optional absolute path as a fallback
    pub abs_path: Option<PathBuf>,
}

impl UndoFileReference {
    /// Create a new file reference by computing both relative and absolute paths
    pub fn new(path: &Path) -> SearchResult<Self> {
        // Get absolute path
        let abs_path = path.canonicalize().map_err(|e| SearchError::IoError(e))?;

        // Find workspace root and compute relative path
        let workspace_root = detect_workspace_root(path)?;
        let workspace_root = workspace_root
            .canonicalize()
            .map_err(|e| SearchError::IoError(e))?;

        // Strip the workspace root to get the relative path
        let rel_path = match abs_path.strip_prefix(&workspace_root) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => {
                // If stripping fails, try with the original path components
                path.strip_prefix(&workspace_root)
                    .unwrap_or_else(|_| Path::new(path.file_name().unwrap_or_default()))
                    .to_path_buf()
            }
        };

        Ok(Self {
            rel_path,
            abs_path: Some(abs_path),
        })
    }

    /// Get a display representation of the path
    pub fn display(&self) -> std::path::Display {
        if let Some(abs) = &self.abs_path {
            abs.display()
        } else {
            self.rel_path.display()
        }
    }

    /// Check if the file exists
    pub fn exists(&self) -> bool {
        if let Some(abs) = &self.abs_path {
            abs.exists()
        } else {
            self.resolve().map(|p| p.exists()).unwrap_or(false)
        }
    }

    /// Resolve the reference to an absolute path using the current workspace root
    pub fn resolve(&self) -> SearchResult<PathBuf> {
        if let Some(abs_path) = self.abs_path.as_ref() {
            Ok(abs_path.clone())
        } else {
            let workspace_root = detect_workspace_root(&self.rel_path)?;
            let abs_path = workspace_root.join(&self.rel_path);
            Ok(abs_path.canonicalize()?)
        }
    }

    /// Get the absolute path, resolving if necessary
    pub fn get_abs_path(&self) -> SearchResult<PathBuf> {
        if let Some(abs_path) = self.abs_path.as_ref() {
            Ok(abs_path.clone())
        } else {
            self.resolve()
        }
    }

    /// Create a new file reference from an existing one, but with a new absolute path
    pub fn with_abs_path(&self, abs_path: PathBuf) -> SearchResult<Self> {
        let workspace_root = detect_workspace_root(&abs_path)?;
        let rel_path = abs_path
            .strip_prefix(&workspace_root)
            .unwrap_or_else(|_| abs_path.as_path())
            .to_path_buf();

        Ok(Self {
            rel_path,
            abs_path: Some(abs_path),
        })
    }

    /// Create a new file reference from an existing one, but with a new relative path
    pub fn with_rel_path(&self, rel_path: PathBuf) -> SearchResult<Self> {
        let abs_path = if let Some(abs) = self.abs_path.as_ref() {
            Some(abs.canonicalize()?)
        } else {
            let workspace_root = detect_workspace_root(&rel_path)?;
            Some(workspace_root.join(&rel_path).canonicalize()?)
        };

        Ok(Self { rel_path, abs_path })
    }
}

impl AsRef<Path> for UndoFileReference {
    fn as_ref(&self) -> &Path {
        if let Some(abs) = &self.abs_path {
            abs.as_ref()
        } else {
            self.rel_path.as_ref()
        }
    }
}

/// A hunk of changes in a file diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// The 1-based starting line in the original file
    pub original_start_line: usize,
    /// The 1-based starting line in the new file
    pub new_start_line: usize,
    /// Number of lines in the original hunk
    pub original_line_count: usize,
    /// Number of lines in the new hunk
    pub new_line_count: usize,
    /// The actual lines removed from the original
    pub original_lines: Vec<String>,
    /// The actual lines that replaced them
    pub new_lines: Vec<String>,
}

/// A diff for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// The path to the modified file
    pub file_path: UndoFileReference,
    /// The hunks of changes made to this file
    pub hunks: Vec<DiffHunk>,
}

/// Information about a replacement operation for undo purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoInfo {
    /// Timestamp when the operation was performed
    pub timestamp: u64,
    /// Description of the operation
    pub description: String,
    /// Map of original files to their backup paths
    pub backups: Vec<(UndoFileReference, UndoFileReference)>,
    /// Size of the operation in bytes
    pub total_size: u64,
    /// Number of files modified
    pub file_count: usize,
    /// Whether the operation was a dry run
    pub dry_run: bool,
    /// Detailed patch-based diffs for each modified file
    #[serde(default)]
    pub file_diffs: Vec<FileDiff>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::init_workspace;
    use std::fs;
    use std::time::SystemTime;
    use tempfile::TempDir;

    #[test]
    fn test_undo_file_reference() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Initialize workspace
        init_workspace(root)?;

        // Create a test file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        // Create file reference
        let file_ref = UndoFileReference::new(&test_file)?;

        // Check paths
        assert_eq!(file_ref.rel_path, PathBuf::from("test.txt"));
        assert!(file_ref.abs_path.is_some());
        assert_eq!(
            file_ref.abs_path.as_ref().unwrap().canonicalize().unwrap(),
            test_file.canonicalize().unwrap()
        );

        // Test resolution
        let resolved = file_ref.resolve()?;
        assert_eq!(resolved.canonicalize()?, test_file.canonicalize()?);

        Ok(())
    }

    #[test]
    fn test_undo_info_serialization() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Initialize workspace
        init_workspace(root)?;

        // Create test files
        let original = root.join("original.txt");
        let backup = root.join("backup.txt");
        fs::write(&original, "test content").unwrap();
        fs::write(&backup, "backup content").unwrap();

        // Create undo info
        let original_ref = UndoFileReference::new(&original)?;
        let backup_ref = UndoFileReference::new(&backup)?;

        let undo_info = UndoInfo {
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            description: "Test replacement".to_string(),
            backups: vec![(original_ref, backup_ref)],
            total_size: 100,
            file_count: 1,
            dry_run: false,
            file_diffs: vec![],
        };

        // Test serialization/deserialization
        let json = serde_json::to_string_pretty(&undo_info)?;
        let deserialized: UndoInfo = serde_json::from_str(&json)?;

        assert_eq!(undo_info.description, deserialized.description);
        assert_eq!(
            undo_info.backups[0].0.rel_path,
            deserialized.backups[0].0.rel_path
        );
        assert_eq!(
            undo_info.backups[0].1.rel_path,
            deserialized.backups[0].1.rel_path
        );

        Ok(())
    }
}
