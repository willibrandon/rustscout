use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::FileSignature;
use crate::errors::{SearchError, SearchResult};

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Renamed(PathBuf),
    Deleted,
    Unchanged,
}

#[derive(Debug)]
pub struct FileChangeInfo {
    pub path: PathBuf,
    pub status: ChangeStatus,
}

/// Trait for implementing different change detection strategies
pub trait ChangeDetector {
    fn detect_changes(&self, paths: &[PathBuf]) -> SearchResult<Vec<FileChangeInfo>>;
}

/// Detects changes using file signatures (mtime + size)
pub struct FileSignatureDetector;

impl FileSignatureDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn compute_signature(path: &Path) -> SearchResult<FileSignature> {
        let metadata = std::fs::metadata(path).map_err(SearchError::IoError)?;

        Ok(FileSignature {
            mtime: metadata.modified().map_err(SearchError::IoError)?,
            size: metadata.len(),
            hash: None,
        })
    }
}

impl Default for FileSignatureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeDetector for FileSignatureDetector {
    fn detect_changes(&self, paths: &[PathBuf]) -> SearchResult<Vec<FileChangeInfo>> {
        let mut changes = Vec::new();

        for path in paths {
            if !path.exists() {
                changes.push(FileChangeInfo {
                    path: path.to_owned(),
                    status: ChangeStatus::Deleted,
                });
                continue;
            }

            // For now, treat all existing files as modified
            // Later we'll compare with cached signatures
            changes.push(FileChangeInfo {
                path: path.to_owned(),
                status: ChangeStatus::Modified,
            });
        }

        Ok(changes)
    }
}

/// Detects changes using git status
pub struct GitStatusDetector {
    root_path: PathBuf,
}

impl GitStatusDetector {
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    fn is_git_repo(&self) -> bool {
        self.root_path.join(".git").exists()
    }
}

impl ChangeDetector for GitStatusDetector {
    fn detect_changes(&self, paths: &[PathBuf]) -> SearchResult<Vec<FileChangeInfo>> {
        if !self.is_git_repo() {
            return Err(SearchError::CacheError("Not a git repository".to_string()));
        }

        let output = Command::new("git")
            .current_dir(&self.root_path)
            .args(["status", "--porcelain"])
            .output()
            .map_err(|e| SearchError::CacheError(format!("Failed to run git status: {}", e)))?;

        if !output.status.success() {
            return Err(SearchError::CacheError(
                "Git status command failed".to_string(),
            ));
        }

        let status_output = String::from_utf8_lossy(&output.stdout);
        let mut changes = Vec::new();

        for line in status_output.lines() {
            if line.len() < 4 {
                continue;
            }

            let status = &line[0..2];
            let file_path = line[3..].trim();
            let path = self.root_path.join(file_path);

            // Only include files that are in our search paths
            if !paths.iter().any(|p| path.starts_with(p)) {
                continue;
            }

            let status = match status {
                "??" => ChangeStatus::Added,
                " M" | "M " | "MM" => ChangeStatus::Modified,
                "R " => {
                    // Handle renamed files
                    if let Some(old_path) = file_path.split("->").next() {
                        ChangeStatus::Renamed(PathBuf::from(old_path.trim()))
                    } else {
                        ChangeStatus::Modified
                    }
                }
                "D " => ChangeStatus::Deleted,
                _ => ChangeStatus::Modified, // Treat other statuses as modified
            };

            changes.push(FileChangeInfo { path, status });
        }

        Ok(changes)
    }
}

/// Factory for creating change detectors
pub fn create_detector(
    strategy: ChangeDetectionStrategy,
    root_path: PathBuf,
) -> Box<dyn ChangeDetector> {
    match strategy {
        ChangeDetectionStrategy::FileSignature => Box::new(FileSignatureDetector::new()),
        ChangeDetectionStrategy::GitStatus => Box::new(GitStatusDetector::new(root_path)),
        ChangeDetectionStrategy::Auto => {
            if Path::new(".git").exists() {
                Box::new(GitStatusDetector::new(root_path))
            } else {
                Box::new(FileSignatureDetector::new())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ChangeDetectionStrategy {
    FileSignature,
    GitStatus,
    Auto,
}
