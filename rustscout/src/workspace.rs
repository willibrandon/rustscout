use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{unify_path, SearchError, SearchResult};

const WORKSPACE_DIR: &str = ".rustscout";
const WORKSPACE_CONFIG: &str = "workspace.json";
const MAX_UPWARD_STEPS: usize = 20;

/// Metadata about a RustScout workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    /// The canonical root path where we anchor relative references
    pub root_path: PathBuf,
    /// Version of the workspace format for future compatibility
    pub version: String,
    /// Format used for workspace configuration (json or yaml)
    pub format: String,
    /// Optional global configuration overrides
    #[serde(default)]
    pub global_config: Option<GlobalConfig>,
}

/// Global configuration that can be stored at the workspace level
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    /// Patterns to ignore across the entire workspace
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Default file extensions to search
    #[serde(default)]
    pub default_extensions: Option<Vec<String>>,
}

impl WorkspaceMetadata {
    /// Create a new workspace metadata instance
    pub fn new(root_path: PathBuf, format: String) -> Self {
        Self {
            root_path,
            version: env!("CARGO_PKG_VERSION").to_string(),
            format,
            global_config: None,
        }
    }

    /// Save workspace metadata to disk
    pub fn save(&self) -> SearchResult<()> {
        let workspace_dir = self.root_path.join(WORKSPACE_DIR);
        fs::create_dir_all(&workspace_dir).map_err(SearchError::IoError)?;

        let config_path = workspace_dir.join(WORKSPACE_CONFIG);
        let json = serde_json::to_string_pretty(self).map_err(|e| SearchError::JsonError(e))?;

        fs::write(config_path, json).map_err(SearchError::IoError)?;
        Ok(())
    }

    /// Load workspace metadata from disk
    pub fn load(root_path: &Path) -> SearchResult<Self> {
        let config_path = root_path.join(WORKSPACE_DIR).join(WORKSPACE_CONFIG);
        if !config_path.exists() {
            return Ok(Self::new(root_path.to_path_buf(), "json".to_string()));
        }

        let json = fs::read_to_string(&config_path).map_err(SearchError::IoError)?;
        let mut metadata: WorkspaceMetadata =
            serde_json::from_str(&json).map_err(|e| SearchError::JsonError(e))?;

        // Always use the provided root path to avoid path inconsistencies
        metadata.root_path = root_path.to_path_buf();
        Ok(metadata)
    }
}

/// Initialize a new workspace at the specified directory
pub fn init_workspace(root: &Path, format: &str) -> SearchResult<WorkspaceMetadata> {
    let root = root.canonicalize()?;
    let rustscout_dir = root.join(WORKSPACE_DIR);
    if !rustscout_dir.exists() {
        fs::create_dir_all(&rustscout_dir)?;
    }

    let metadata = WorkspaceMetadata::new(root.to_path_buf(), format.to_string());

    // Save in the specified format
    let config_path = rustscout_dir.join(match format.to_lowercase().as_str() {
        "yaml" => "workspace.yaml",
        _ => WORKSPACE_CONFIG, // Default to JSON
    });

    if format.to_lowercase() == "yaml" {
        let yaml = serde_yaml::to_string(&metadata).map_err(|e| {
            SearchError::config_error(format!(
                "Failed to serialize workspace metadata to YAML: {}",
                e
            ))
        })?;
        fs::write(&config_path, yaml)?;
    } else {
        let json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&config_path, json)?;
    }

    Ok(metadata)
}

/// Detect a workspace root by walking upward from the starting directory.
/// If no workspace is found, returns the starting directory without creating one.
pub fn detect_workspace_root(starting_dir: &Path) -> SearchResult<PathBuf> {
    let mut current = unify_path(starting_dir);

    // Walk up the directory tree looking for .rustscout
    for _ in 0..MAX_UPWARD_STEPS {
        let workspace_marker = current.join(WORKSPACE_DIR);
        if workspace_marker.exists() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }

    // If no workspace found, use starting directory without creating one
    Ok(unify_path(starting_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_workspace_detection() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a nested directory structure
        let nested = root.join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();

        // No workspace exists yet, should use nested as root
        let detected = detect_workspace_root(&nested)?;
        assert_eq!(unify_path(&nested), detected);

        // Create workspace at root/a
        let workspace_root = root.join("a");
        init_workspace(&workspace_root, "json")?;

        // Should now detect workspace at root/a
        let detected = detect_workspace_root(&nested)?;
        assert_eq!(unify_path(&workspace_root), detected);

        Ok(())
    }

    #[test]
    fn test_workspace_metadata() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create initial metadata
        let mut metadata = WorkspaceMetadata::new(root.to_path_buf(), "json".to_string());
        metadata.global_config = Some(GlobalConfig {
            ignore_patterns: vec!["*.tmp".to_string()],
            default_extensions: Some(vec!["rs".to_string()]),
        });

        // Save and reload
        metadata.save()?;
        let loaded = WorkspaceMetadata::load(root)?;

        assert_eq!(metadata.version, loaded.version);
        assert_eq!(
            metadata.global_config.as_ref().unwrap().ignore_patterns,
            loaded.global_config.as_ref().unwrap().ignore_patterns
        );

        Ok(())
    }

    #[test]
    fn test_workspace_initialization() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Initialize new workspace
        let metadata = init_workspace(root, "json")?;
        assert_eq!(unify_path(&metadata.root_path), unify_path(root));

        // Verify workspace directory and config exist
        assert!(root.join(WORKSPACE_DIR).exists());
        assert!(root.join(WORKSPACE_DIR).join(WORKSPACE_CONFIG).exists());

        Ok(())
    }
}
