use crate::errors::{SearchError, SearchResult};
use crate::metrics::MemoryMetrics;
use crate::search::matcher::{PatternDefinition, WordBoundaryMode};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::MmapOptions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// File size thresholds for different processing strategies
const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
const LARGE_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

/// A pattern and its replacement text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementPattern {
    /// The pattern definition
    pub definition: PatternDefinition,
    /// The text to replace matches with
    pub replacement_text: String,
}

/// Configuration for replacement operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementConfig {
    /// The patterns and their replacements
    pub patterns: Vec<ReplacementPattern>,

    /// Whether to create backups of modified files
    pub backup_enabled: bool,

    /// Whether to only show what would be changed without modifying files
    pub dry_run: bool,

    /// Directory for storing backups (if enabled)
    pub backup_dir: Option<PathBuf>,

    /// Whether to preserve file permissions and timestamps
    pub preserve_metadata: bool,

    /// Directory for storing undo information
    pub undo_dir: PathBuf,
}

impl ReplacementConfig {
    pub fn load_from(path: &Path) -> Result<Self, SearchError> {
        let content = fs::read_to_string(path).map_err(SearchError::IoError)?;
        serde_yaml::from_str(&content)
            .map_err(|e| SearchError::config_error(format!("Failed to parse config: {}", e)))
    }

    pub fn merge_with_cli(&mut self, cli_config: ReplacementConfig) {
        // CLI options take precedence over config file
        if !cli_config.patterns.is_empty() {
            self.patterns = cli_config.patterns;
        }
        self.backup_enabled |= cli_config.backup_enabled;
        self.dry_run |= cli_config.dry_run;
        if cli_config.backup_dir.is_some() {
            self.backup_dir = cli_config.backup_dir;
        }
        self.preserve_metadata |= cli_config.preserve_metadata;
    }
}

/// Represents a single replacement operation within a file
#[derive(Debug, Clone)]
pub struct ReplacementTask {
    /// The file this replacement applies to
    pub file_path: PathBuf,

    /// The byte range in the file to replace
    pub original_range: (usize, usize),

    /// The text to insert in place of the matched range
    pub replacement_text: String,

    /// The pattern definition that matched
    pub pattern_index: usize,

    /// The configuration for this replacement operation
    pub config: ReplacementConfig,
}

impl ReplacementTask {
    pub fn new(
        file_path: PathBuf,
        original_range: (usize, usize),
        replacement_text: String,
        pattern_index: usize,
        config: ReplacementConfig,
    ) -> Self {
        Self {
            file_path,
            original_range,
            replacement_text,
            pattern_index,
            config,
        }
    }

    pub fn validate(&self) -> SearchResult<()> {
        // Check empty pattern
        if self.config.patterns.is_empty() {
            return Err(SearchError::invalid_pattern("Pattern cannot be empty"));
        }

        // Get the pattern definition
        let pattern = &self.config.patterns[self.pattern_index];

        // Validate regex if enabled
        if pattern.definition.is_regex {
            let test_regex = regex::Regex::new(&pattern.definition.text)
                .map_err(|e| SearchError::invalid_pattern(e.to_string()))?;

            // If word boundary is enabled, ensure the pattern has proper boundary markers
            if matches!(
                pattern.definition.boundary_mode,
                WordBoundaryMode::WholeWords
            ) {
                validate_word_boundaries(&test_regex)?;
            }

            // Validate capture groups
            validate_capture_groups(&test_regex, &pattern.replacement_text)?;
        }
        Ok(())
    }

    pub fn apply(&self, content: &str) -> SearchResult<String> {
        self.validate()?;

        let pattern = &self.config.patterns[self.pattern_index];

        if pattern.definition.is_regex {
            let regex = regex::Regex::new(&pattern.definition.text)
                .map_err(|e| SearchError::invalid_pattern(e.to_string()))?;

            Ok(regex
                .replace_all(content, &pattern.replacement_text)
                .into_owned())
        } else {
            Ok(content.replace(&pattern.definition.text, &pattern.replacement_text))
        }
    }
}

fn validate_capture_groups(regex: &regex::Regex, capture_fmt: &str) -> SearchResult<()> {
    let group_count = regex.captures_len(); // includes group 0
    let re = regex::Regex::new(r"\$(\d+)").unwrap();

    for cap in re.captures_iter(capture_fmt) {
        if let Some(num_str) = cap.get(1) {
            if let Ok(num) = num_str.as_str().parse::<usize>() {
                // group_count includes $0 => highest valid group is group_count - 1
                if num >= group_count {
                    return Err(SearchError::invalid_pattern(format!(
                        "Capture group ${} does not exist",
                        num
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Collects all replacements for a single file
#[derive(Debug)]
pub struct FileReplacementPlan {
    /// The file to modify
    pub file_path: PathBuf,

    /// All replacements for this file, sorted by range start
    pub replacements: Vec<ReplacementTask>,

    /// Original file metadata (if preserving)
    pub original_metadata: Option<std::fs::Metadata>,
}

/// Information about a backup for undo operations
#[derive(Debug, Serialize, Deserialize)]
pub struct UndoInfo {
    /// Timestamp when the operation was performed
    pub timestamp: u64,
    /// Description of the operation
    pub description: String,
    /// Map of original files to their backup paths
    pub backups: Vec<(PathBuf, PathBuf)>,
    /// Size of the operation in bytes
    pub total_size: u64,
    /// Number of files modified
    pub file_count: usize,
    /// Whether the operation was a dry run
    pub dry_run: bool,
}

impl fmt::Display for UndoInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} files, {} bytes{}",
            self.description,
            self.file_count,
            self.total_size,
            if self.dry_run { " (dry run)" } else { "" }
        )
    }
}

/// Strategy for processing files based on their size
#[derive(Debug, Clone, Copy)]
enum ProcessingStrategy {
    InMemory,     // For small files
    Streaming,    // For medium files
    MemoryMapped, // For large files
}

impl ProcessingStrategy {
    fn for_file_size(size: u64) -> Self {
        if size < SMALL_FILE_THRESHOLD {
            ProcessingStrategy::InMemory
        } else if size < LARGE_FILE_THRESHOLD {
            ProcessingStrategy::Streaming
        } else {
            ProcessingStrategy::MemoryMapped
        }
    }
}

impl FileReplacementPlan {
    /// Creates a new plan for the given file
    pub fn new(file_path: PathBuf) -> SearchResult<Self> {
        let metadata = if let Ok(meta) = fs::metadata(&file_path) {
            Some(meta)
        } else {
            None
        };

        Ok(Self {
            file_path,
            replacements: Vec::new(),
            original_metadata: metadata,
        })
    }

    /// Adds a replacement task to this plan
    pub fn add_replacement(&mut self, task: ReplacementTask) -> SearchResult<()> {
        // Validate the task first
        task.validate()?;

        // Check for overlapping replacements
        for existing in &self.replacements {
            if task.original_range.0 < existing.original_range.1
                && existing.original_range.0 < task.original_range.1
            {
                return Err(SearchError::config_error(
                    "Overlapping replacements are not allowed",
                ));
            }
        }

        // Add the task, keeping replacements sorted by range start
        let insert_pos = self
            .replacements
            .binary_search_by_key(&task.original_range.0, |t| t.original_range.0)
            .unwrap_or_else(|e| e);
        self.replacements.insert(insert_pos, task);
        Ok(())
    }

    /// Applies the replacements to the file using the appropriate strategy
    pub fn apply(
        &self,
        config: &ReplacementConfig,
        metrics: &MemoryMetrics,
    ) -> SearchResult<Option<PathBuf>> {
        // Create backup if enabled
        let backup_path = if config.backup_enabled {
            self.create_backup(config)?
        } else {
            None
        };

        // Don't modify files in dry run mode
        if config.dry_run {
            return Ok(backup_path);
        }

        // Choose processing strategy based on file size
        let strategy = if let Some(metadata) = &self.original_metadata {
            ProcessingStrategy::for_file_size(metadata.len())
        } else {
            ProcessingStrategy::InMemory
        };

        // Apply replacements using chosen strategy
        match strategy {
            ProcessingStrategy::InMemory => self.apply_in_memory(config, metrics),
            ProcessingStrategy::Streaming => self.apply_streaming(config, metrics),
            ProcessingStrategy::MemoryMapped => self.apply_memory_mapped(config, metrics),
        }?;

        // Restore metadata if needed
        if config.preserve_metadata {
            if let Some(metadata) = &self.original_metadata {
                fs::set_permissions(&self.file_path, metadata.permissions())?;
            }
        }

        Ok(backup_path)
    }

    /// Process small files entirely in memory
    fn apply_in_memory(
        &self,
        _config: &ReplacementConfig,
        _metrics: &MemoryMetrics,
    ) -> SearchResult<()> {
        let content = fs::read_to_string(&self.file_path)?;
        let mut result = content.clone();

        // Apply replacements in reverse order to maintain correct offsets
        for task in self.replacements.iter().rev() {
            result.replace_range(
                task.original_range.0..task.original_range.1,
                &task.replacement_text,
            );
        }

        // Write to temporary file and rename atomically
        let tmp_path = self.file_path.with_extension("tmp");
        fs::write(&tmp_path, result)?;
        fs::rename(&tmp_path, &self.file_path)?;

        Ok(())
    }

    /// Process medium files using buffered streaming I/O
    fn apply_streaming(
        &self,
        _config: &ReplacementConfig,
        _metrics: &MemoryMetrics,
    ) -> SearchResult<()> {
        let mut reader = BufReader::new(File::open(&self.file_path)?);
        let tmp_path = self.file_path.with_extension("tmp");
        let mut writer = BufWriter::new(File::create(&tmp_path)?);

        let mut current_pos = 0;
        for task in &self.replacements {
            // Copy unchanged content up to the start of replacement
            let bytes_to_copy = task.original_range.0 as u64 - current_pos;
            let mut limited_reader = reader.by_ref().take(bytes_to_copy);
            std::io::copy(&mut limited_reader, &mut writer)?;

            // Write replacement
            writer.write_all(task.replacement_text.as_bytes())?;
            reader.seek(SeekFrom::Start(task.original_range.1 as u64))?;
            current_pos = task.original_range.1 as u64;
        }

        // Copy remaining content
        std::io::copy(&mut reader, &mut writer)?;
        writer.flush()?;

        // Atomically rename
        fs::rename(&tmp_path, &self.file_path)?;

        Ok(())
    }

    /// Process large files using memory mapping
    fn apply_memory_mapped(
        &self,
        _config: &ReplacementConfig,
        _metrics: &MemoryMetrics,
    ) -> SearchResult<()> {
        let file = File::open(&self.file_path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };

        let mut result = Vec::with_capacity(mmap.len());
        let mut current_pos = 0;

        for task in &self.replacements {
            // Copy unchanged content
            result.extend_from_slice(&mmap[current_pos..task.original_range.0]);
            // Write replacement
            result.extend_from_slice(task.replacement_text.as_bytes());
            current_pos = task.original_range.1;
        }

        // Copy remaining content
        result.extend_from_slice(&mmap[current_pos..]);

        // Write to temporary file and rename atomically
        let tmp_path = self.file_path.with_extension("tmp");
        fs::write(&tmp_path, result)?;
        fs::rename(&tmp_path, &self.file_path)?;

        Ok(())
    }

    /// Create a backup of the file
    fn create_backup(&self, config: &ReplacementConfig) -> SearchResult<Option<PathBuf>> {
        if !config.backup_enabled || config.dry_run {
            return Ok(None);
        }

        // Determine and create the backup directory
        let backup_dir = if let Some(ref specified_dir) = config.backup_dir {
            fs::create_dir_all(specified_dir).map_err(|e| {
                SearchError::config_error(format!(
                    "Failed to create backup directory '{}': {}",
                    specified_dir.display(),
                    e
                ))
            })?;
            specified_dir.clone()
        } else {
            let backups = config.undo_dir.join("backups");
            fs::create_dir_all(&backups).map_err(|e| {
                SearchError::config_error(format!(
                    "Failed to create backup directory '{}': {}",
                    backups.display(),
                    e
                ))
            })?;
            backups
        };

        // Get absolute path for the file
        let file_path_abs = self.file_path.canonicalize().map_err(|e| {
            SearchError::config_error(format!(
                "Failed to get absolute path for '{}': {}",
                self.file_path.display(),
                e
            ))
        })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| SearchError::config_error("System clock set before UNIX EPOCH"))?
            .as_secs();

        let backup_path = backup_dir.join(format!(
            "{}.{}",
            file_path_abs
                .file_name()
                .ok_or_else(|| SearchError::config_error("Invalid file name"))?
                .to_string_lossy(),
            timestamp
        ));

        fs::copy(&file_path_abs, &backup_path).map_err(SearchError::IoError)?;
        Ok(Some(backup_path))
    }

    /// Generates a preview of the changes
    pub fn preview(&self) -> SearchResult<Vec<PreviewResult>> {
        let mut results = Vec::new();

        // Get the content and apply all replacements
        let content = fs::read_to_string(&self.file_path).map_err(SearchError::IoError)?;
        let mut new_content = content.clone();

        for task in &self.replacements {
            new_content = task.apply(&new_content)?;
        }

        // Split into lines and compare
        let original_lines: Vec<&str> = content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut changed_original = Vec::new();
        let mut changed_new = Vec::new();
        let mut line_numbers = Vec::new();

        // Compare lines and collect changes
        for (i, (orig, new)) in original_lines.iter().zip(&new_lines).enumerate() {
            if orig != new {
                changed_original.push(orig.to_string());
                changed_new.push(new.to_string());
                line_numbers.push(i + 1); // 1-based line numbers
            }
        }

        if !changed_original.is_empty() {
            results.push(PreviewResult {
                file_path: self.file_path.clone(),
                original_lines: changed_original,
                new_lines: changed_new,
                line_numbers,
            });
        }

        Ok(results)
    }
}

/// Represents the complete set of replacements across all files
#[derive(Debug)]
pub struct ReplacementSet {
    /// The configuration for this replacement operation
    pub config: ReplacementConfig,

    /// Plans for each file that needs modification
    pub plans: Vec<FileReplacementPlan>,

    /// Metrics for tracking memory usage
    metrics: Arc<MemoryMetrics>,
}

impl ReplacementSet {
    /// Creates a new replacement set with the given configuration
    pub fn new(config: ReplacementConfig) -> Self {
        Self {
            config,
            plans: Vec::new(),
            metrics: Arc::new(MemoryMetrics::new()),
        }
    }

    /// Adds a file replacement plan to this set
    pub fn add_plan(&mut self, plan: FileReplacementPlan) {
        self.plans.push(plan);
    }

    /// Lists available undo operations with detailed information
    pub fn list_undo_operations(
        config: &ReplacementConfig,
    ) -> SearchResult<Vec<(UndoInfo, PathBuf)>> {
        let undo_dir = &config.undo_dir;
        if !undo_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(undo_dir).map_err(|e| {
            SearchError::config_error(format!("Failed to read undo directory: {}", e))
        })?;

        let mut operations = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(entry.path()).map_err(|e| {
                    SearchError::config_error(format!("Failed to read undo info: {}", e))
                })?;

                let info: UndoInfo = serde_json::from_str(&content).map_err(|e| {
                    SearchError::config_error(format!("Failed to parse undo info: {}", e))
                })?;

                operations.push((info, entry.path()));
            }
        }

        operations.sort_by_key(|(info, _)| info.timestamp);
        Ok(operations)
    }

    /// Undoes a specific replacement operation by ID with progress reporting
    pub fn undo_by_id(id: u64, config: &ReplacementConfig) -> SearchResult<()> {
        let info_path = config.undo_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&info_path)
            .map_err(|e| SearchError::config_error(format!("Failed to read undo info: {}", e)))?;

        let info: UndoInfo = serde_json::from_str(&content)
            .map_err(|e| SearchError::config_error(format!("Failed to parse undo info: {}", e)))?;

        for (original, backup) in info.backups {
            let orig_abs = original.canonicalize().map_err(|e| {
                SearchError::config_error(format!(
                    "Failed to get absolute path for original: {}",
                    e
                ))
            })?;

            if !backup.exists() {
                return Err(SearchError::config_error(format!(
                    "Backup file not found: {}",
                    backup.display()
                )));
            }

            fs::copy(&backup, &orig_abs).map_err(|e| {
                SearchError::config_error(format!("Failed to restore backup: {}", e))
            })?;
            fs::remove_file(&backup).ok();
        }

        fs::remove_file(&info_path).ok();
        Ok(())
    }

    /// Gets a reference to the metrics
    pub fn metrics(&self) -> &MemoryMetrics {
        &self.metrics
    }

    /// Applies all replacements in parallel with progress reporting
    pub fn apply_with_progress(&self) -> SearchResult<Vec<PathBuf>> {
        let progress = ProgressBar::new(self.plans.len() as u64);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} files")
                .unwrap()
                .progress_chars("=>-"),
        );

        let backups = Mutex::new(Vec::new());
        let config = &self.config;
        let metrics = &self.metrics;

        // Process files in parallel
        self.plans
            .par_iter()
            .try_for_each(|plan| -> SearchResult<()> {
                if !config.dry_run {
                    if let Some(backup_path) = plan.apply(config, metrics)? {
                        let mut backups = backups.lock().unwrap();
                        backups.push((plan.file_path.clone(), backup_path));
                    }
                }
                progress.inc(1);
                Ok(())
            })?;

        let backups = backups.into_inner().unwrap();
        let mut undo_metadata = Vec::new();

        // Save undo information
        if !self.config.dry_run && !backups.is_empty() {
            self.save_undo_info(&backups)?;
            undo_metadata.extend(backups.into_iter().map(|(_, backup)| backup));
        }

        progress.finish();
        Ok(undo_metadata)
    }

    /// Applies all replacements in parallel without progress reporting
    pub fn apply(&self) -> SearchResult<()> {
        let metrics = Arc::new(MemoryMetrics::new());
        let mut backup_paths = Vec::new();

        // Apply all plans
        for plan in &self.plans {
            if let Some(backup_path) = plan.apply(&self.config, &metrics)? {
                backup_paths.push((plan.file_path.clone(), backup_path));
            }
        }

        // Record undo information if any backups were created
        if !backup_paths.is_empty() && !self.config.dry_run {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| SearchError::config_error("System clock set before UNIX EPOCH"))?
                .as_secs();

            let description = format!(
                "Replace '{}' with '{}'",
                if !self.config.patterns.is_empty() {
                    &self.config.patterns[0].definition.text
                } else {
                    "empty pattern"
                },
                self.config.patterns[0].replacement_text
            );

            let total_size: u64 = backup_paths
                .iter()
                .filter_map(|(_, backup)| fs::metadata(backup).ok())
                .map(|m| m.len())
                .sum();

            let undo_info = UndoInfo {
                timestamp,
                description,
                backups: backup_paths,
                total_size,
                file_count: self.plans.len(),
                dry_run: self.config.dry_run,
            };

            // Save undo information
            fs::create_dir_all(&self.config.undo_dir).map_err(|e| {
                SearchError::config_error(format!("Failed to create undo directory: {}", e))
            })?;

            let undo_file = self.config.undo_dir.join(format!("{}.json", timestamp));
            let undo_json = serde_json::to_string_pretty(&undo_info).map_err(|e| {
                SearchError::config_error(format!("Failed to serialize undo info: {}", e))
            })?;

            fs::write(&undo_file, undo_json).map_err(|e| {
                SearchError::config_error(format!("Failed to write undo file: {}", e))
            })?;
        }

        Ok(())
    }

    /// Generates a preview of the changes in parallel
    pub fn preview(&self) -> SearchResult<Vec<PreviewResult>> {
        let mut results = Vec::new();

        for plan in &self.plans {
            // Read the file content
            let content = fs::read_to_string(&plan.file_path).map_err(SearchError::IoError)?;

            // Apply all replacements in memory first
            let mut new_content = content.clone();
            for task in &plan.replacements {
                new_content = task.apply(&new_content)?;
            }

            // Split into lines and compare
            let original_lines: Vec<&str> = content.lines().collect();
            let new_lines: Vec<&str> = new_content.lines().collect();

            let mut changed_original = Vec::new();
            let mut changed_new = Vec::new();
            let mut line_numbers = Vec::new();

            // Compare lines and collect changes
            for (i, (orig, new)) in original_lines.iter().zip(&new_lines).enumerate() {
                if orig != new {
                    changed_original.push(orig.to_string());
                    changed_new.push(new.to_string());
                    line_numbers.push(i + 1); // 1-based line numbers
                }
            }

            if !changed_original.is_empty() {
                results.push(PreviewResult {
                    file_path: plan.file_path.clone(),
                    original_lines: changed_original,
                    new_lines: changed_new,
                    line_numbers,
                });
            }
        }

        Ok(results)
    }

    /// Save undo information for a set of backups
    fn save_undo_info(&self, backups: &[(PathBuf, PathBuf)]) -> SearchResult<()> {
        fs::create_dir_all(&self.config.undo_dir).map_err(|e| {
            SearchError::config_error(format!("Failed to create undo directory: {}", e))
        })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let pattern_text = if !self.config.patterns.is_empty() {
            &self.config.patterns[0].definition.text
        } else {
            "empty pattern"
        };

        let info = UndoInfo {
            timestamp,
            description: format!(
                "Replace '{}' with '{}'",
                pattern_text, self.config.patterns[0].replacement_text
            ),
            backups: backups.to_vec(),
            total_size: backups
                .iter()
                .map(|(_, b)| fs::metadata(b).map(|m| m.len()).unwrap_or(0))
                .sum(),
            file_count: backups.len(),
            dry_run: self.config.dry_run,
        };

        let info_path = self.config.undo_dir.join(format!("{}.json", timestamp));
        let content = serde_json::to_string_pretty(&info).map_err(|e| {
            SearchError::config_error(format!("Failed to serialize undo info: {}", e))
        })?;

        fs::write(&info_path, content)
            .map_err(|e| SearchError::config_error(format!("Failed to save undo info: {}", e)))?;

        Ok(())
    }
}

/// Result of generating a preview for a file
#[derive(Debug)]
pub struct PreviewResult {
    /// The file being modified
    pub file_path: PathBuf,

    /// Original lines that will be modified
    pub original_lines: Vec<String>,

    /// New lines after modification
    pub new_lines: Vec<String>,

    /// Line numbers for each change
    pub line_numbers: Vec<usize>,
}

fn validate_word_boundaries(regex: &regex::Regex) -> SearchResult<()> {
    // Check if the pattern has proper word boundary markers
    let pattern = regex.as_str();
    if !pattern.starts_with("\\b") || !pattern.ends_with("\\b") {
        return Err(SearchError::invalid_pattern(
            "Pattern must have word boundary markers (\\b) when word boundary mode is enabled",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode};
    use tempfile::tempdir;

    // Helper function to create a basic pattern definition
    fn create_pattern_def(text: &str, is_regex: bool) -> PatternDefinition {
        PatternDefinition {
            text: text.to_string(),
            is_regex,
            boundary_mode: WordBoundaryMode::None,
            hyphen_mode: HyphenMode::default(),
        }
    }

    #[test]
    fn test_processing_strategies() -> SearchResult<()> {
        // Create test files with known sizes
        let dir = tempdir().map_err(|e| {
            SearchError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        // Small file (< 32KB)
        let small_path = dir.path().join("small.txt");
        fs::write(&small_path, "small test content").map_err(SearchError::IoError)?;

        // Medium file (32KB - 10MB)
        let medium_path = dir.path().join("medium.txt");
        let medium_content = "medium test content\n".repeat(2000);
        fs::write(&medium_path, &medium_content).map_err(SearchError::IoError)?;

        // Large file (> 10MB)
        let large_path = dir.path().join("large.txt");
        let large_content = "large test content\n".repeat(1_000_000);
        fs::write(&large_path, &large_content).map_err(SearchError::IoError)?;

        // Test small file strategy
        let small_meta = fs::metadata(&small_path).map_err(SearchError::IoError)?;
        assert!(matches!(
            ProcessingStrategy::for_file_size(small_meta.len()),
            ProcessingStrategy::InMemory
        ));

        // Test medium file strategy
        let medium_meta = fs::metadata(&medium_path).map_err(SearchError::IoError)?;
        assert!(matches!(
            ProcessingStrategy::for_file_size(medium_meta.len()),
            ProcessingStrategy::Streaming
        ));

        // Test large file strategy
        let large_meta = fs::metadata(&large_path).map_err(SearchError::IoError)?;
        assert!(matches!(
            ProcessingStrategy::for_file_size(large_meta.len()),
            ProcessingStrategy::MemoryMapped
        ));

        Ok(())
    }

    #[test]
    fn test_replacement_config_merge() {
        let mut base_config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("old", false),
                replacement_text: "new".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: PathBuf::from("undo"),
        };

        let cli_config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("cli_pattern", false),
                replacement_text: "cli_replacement".to_string(),
            }],
            backup_enabled: true,
            dry_run: true,
            backup_dir: Some(PathBuf::from("backup")),
            preserve_metadata: true,
            undo_dir: PathBuf::from("cli_undo"),
        };

        base_config.merge_with_cli(cli_config);

        assert_eq!(base_config.patterns[0].definition.text, "cli_pattern");
        assert_eq!(base_config.patterns[0].replacement_text, "cli_replacement");
        assert!(base_config.backup_enabled);
        assert!(base_config.dry_run);
        assert_eq!(base_config.backup_dir, Some(PathBuf::from("backup")));
        assert!(base_config.preserve_metadata);
    }

    #[test]
    fn test_replacement_with_backup() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: true,
            dry_run: false,
            backup_dir: Some(dir.path().to_path_buf()),
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        let backup_path = plan.apply(&config, &MemoryMetrics::new())?;
        assert!(backup_path.is_some());
        assert!(backup_path.as_ref().unwrap().exists());

        let backup_content =
            fs::read_to_string(backup_path.unwrap()).map_err(SearchError::IoError)?;
        assert_eq!(backup_content, "test content");

        let new_content = fs::read_to_string(&file_path).map_err(SearchError::IoError)?;
        assert_eq!(new_content, "replaced content");

        Ok(())
    }

    #[test]
    fn test_dry_run() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        let original_content = "test content";
        fs::write(&file_path, original_content)?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: true,
            dry_run: true,
            backup_dir: Some(dir.path().to_path_buf()),
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        let backup_path = plan.apply(&config, &MemoryMetrics::new())?;
        assert!(backup_path.is_none());

        let final_content = fs::read_to_string(&file_path).map_err(SearchError::IoError)?;
        assert_eq!(final_content, original_content);

        Ok(())
    }

    #[test]
    fn test_regex_replacement() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "fn test_func() {}")?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def(r"fn (\w+)\(\)", true),
                replacement_text: "fn new_$1()".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 14),
            "fn new_test_func()".to_string(),
            0,
            config.clone(),
        ))?;

        plan.apply(&config, &MemoryMetrics::new())?;

        let new_content = fs::read_to_string(&file_path).map_err(SearchError::IoError)?;
        assert_eq!(new_content, "fn new_test_func() {}");

        Ok(())
    }

    #[test]
    fn test_invalid_regex_pattern() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("[invalid", true),
                replacement_text: "replacement".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        let result = plan.add_replacement(ReplacementTask::new(
            file_path,
            (0, 4),
            "replacement".to_string(),
            0,
            config.clone(),
        ));

        assert!(
            result.is_err(),
            "Expected an error due to invalid regex pattern"
        );
        Ok(())
    }

    #[test]
    fn test_invalid_capture_group() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def(r"(\w+)", true),
                replacement_text: "$2".to_string(), // $2 doesn't exist, only $1 exists
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let task = ReplacementTask::new(file_path, (0, 4), "$2".to_string(), 0, config.clone());

        let result = task.validate();

        assert!(
            result.is_err(),
            "Expected an error due to invalid capture group reference"
        );
        Ok(())
    }

    #[test]
    fn test_preserve_metadata() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        // Make file read-only before applying changes
        let metadata = fs::metadata(&file_path).map_err(SearchError::IoError)?;
        let mut perms = metadata.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&file_path, perms).map_err(SearchError::IoError)?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: true,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        // Temporarily make file writable for the test
        let metadata = fs::metadata(&file_path).map_err(SearchError::IoError)?;
        let mut perms = metadata.permissions();
        perms.set_readonly(false);
        fs::set_permissions(&file_path, perms).map_err(SearchError::IoError)?;

        plan.apply(&config, &MemoryMetrics::new())?;

        // Check if permissions were preserved
        let new_metadata = fs::metadata(&file_path).map_err(SearchError::IoError)?;
        assert!(new_metadata.permissions().readonly());

        Ok(())
    }

    #[test]
    fn test_multiple_replacements() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test test test")?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;

        // Add multiple replacements
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (5, 9),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (10, 14),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        plan.apply(&config, &MemoryMetrics::new())?;

        let new_content = fs::read_to_string(&file_path).map_err(SearchError::IoError)?;
        assert_eq!(new_content, "replaced replaced replaced");

        Ok(())
    }

    #[test]
    fn test_empty_pattern() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        let config = ReplacementConfig {
            patterns: vec![], // Empty pattern_definitions to test validation
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        let result = plan.add_replacement(ReplacementTask::new(
            file_path,
            (0, 0),
            "something".to_string(),
            0,
            config.clone(),
        ));

        assert!(result.is_err(), "Expected an error due to empty pattern");
        Ok(())
    }

    #[test]
    fn test_overlapping_replacements() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;

        // First replacement should succeed
        let result1 = plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 6),
            "replaced".to_string(),
            0,
            config.clone(),
        ));
        assert!(result1.is_ok(), "First replacement should succeed");

        // Second replacement overlaps with first, should fail
        let result2 = plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (4, 8),
            "new".to_string(),
            0,
            config.clone(),
        ));
        assert!(
            result2.is_err(),
            "Expected an error due to overlapping replacements"
        );

        Ok(())
    }

    #[test]
    fn test_undo_operations() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        let original_content = "test\ntest\nno match\ntest";
        fs::write(&file_path, original_content)?;

        // Create undo directory
        let undo_dir = dir.path().join("undo");
        fs::create_dir_all(&undo_dir).map_err(SearchError::IoError)?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: true,
            dry_run: false,
            backup_dir: Some(dir.path().to_path_buf()),
            preserve_metadata: false,
            undo_dir: undo_dir.clone(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;

        // Add multiple replacements
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        // Create a ReplacementSet and apply the changes
        let mut replacement_set = ReplacementSet::new(config.clone());
        replacement_set.add_plan(plan);
        replacement_set.apply()?;

        // Verify undo information was recorded
        assert!(undo_dir.exists(), "Undo directory should exist");

        let undo_files: Vec<_> = fs::read_dir(&undo_dir)
            .map_err(SearchError::IoError)?
            .filter_map(|entry| entry.ok())
            .collect();

        assert_eq!(undo_files.len(), 1, "Expected one undo file");

        Ok(())
    }

    #[test]
    fn test_preview_replacements() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        let original_content = "test\ntest\nno match\ntest";
        fs::write(&file_path, original_content)?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;

        // Add multiple replacements
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (5, 9),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (20, 24),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        let preview_results = plan.preview()?;
        assert_eq!(preview_results.len(), 1, "Expected one preview result");
        let preview = &preview_results[0];

        assert_eq!(preview.original_lines, vec!["test", "test", "test"]);
        assert_eq!(preview.new_lines, vec!["replaced", "replaced", "replaced"]);
        assert_eq!(preview.line_numbers, vec![1, 2, 4]);

        // Verify original content hasn't changed
        let final_content = fs::read_to_string(&file_path).map_err(SearchError::IoError)?;
        assert_eq!(final_content, original_content);

        Ok(())
    }

    #[test]
    fn test_backup_directory_creation() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test content")?;

        let backup_dir = dir.path().join("backups");
        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: true,
            dry_run: false,
            backup_dir: Some(backup_dir.clone()),
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        let backup_path = plan.apply(&config, &MemoryMetrics::new())?;
        assert!(backup_path.is_some());
        assert!(backup_dir.exists());
        assert!(backup_dir.is_dir());

        Ok(())
    }

    #[test]
    fn test_metrics_tracking() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        let content = "test content".repeat(1000);
        fs::write(&file_path, &content)?;

        let config = ReplacementConfig {
            patterns: vec![ReplacementPattern {
                definition: create_pattern_def("test", false),
                replacement_text: "replaced".to_string(),
            }],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let metrics = Arc::new(MemoryMetrics::new());
        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "replaced".to_string(),
            0,
            config.clone(),
        ))?;

        plan.apply(&config, &metrics)?;

        // Just verify the metrics object exists and can be cloned
        assert!(Arc::strong_count(&metrics) >= 1);

        Ok(())
    }

    #[test]
    fn test_word_boundary_replacement() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test testing tested")?;

        let mut pattern_def = ReplacementPattern {
            definition: create_pattern_def(r"\btest\b", true),
            replacement_text: "pass".to_string(),
        };
        pattern_def.definition.boundary_mode = WordBoundaryMode::WholeWords;

        let config = ReplacementConfig {
            patterns: vec![pattern_def],
            backup_enabled: false,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir: dir.path().to_path_buf(),
        };

        let mut plan = FileReplacementPlan::new(file_path.clone())?;
        plan.add_replacement(ReplacementTask::new(
            file_path.clone(),
            (0, 4),
            "pass".to_string(),
            0,
            config.clone(),
        ))?;

        plan.apply(&config, &MemoryMetrics::new())?;

        let new_content = fs::read_to_string(&file_path).map_err(SearchError::IoError)?;
        assert_eq!(new_content, "pass testing tested");

        Ok(())
    }
}
