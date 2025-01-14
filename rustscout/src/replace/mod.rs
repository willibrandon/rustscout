use crate::errors::{SearchError, SearchResult};
use crate::metrics::MemoryMetrics;
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

/// Configuration for replacement operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementConfig {
    /// The pattern to search for (supports regex)
    pub pattern: String,

    /// The text to replace matches with
    pub replacement: String,

    /// Whether the pattern is a regex
    pub is_regex: bool,

    /// Whether to create backups of modified files
    pub backup_enabled: bool,

    /// Whether to only show what would be changed without modifying files
    pub dry_run: bool,

    /// Directory for storing backups (if enabled)
    pub backup_dir: Option<PathBuf>,

    /// Whether to preserve file permissions and timestamps
    pub preserve_metadata: bool,

    /// Capture groups for regex replacement
    pub capture_groups: Option<String>,

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
        if !cli_config.pattern.is_empty() {
            self.pattern = cli_config.pattern;
        }
        if !cli_config.replacement.is_empty() {
            self.replacement = cli_config.replacement;
        }
        if cli_config.capture_groups.is_some() {
            self.capture_groups = cli_config.capture_groups;
        }
        self.is_regex |= cli_config.is_regex;
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

    /// The configuration for this replacement operation
    pub config: ReplacementConfig,
}

impl ReplacementTask {
    pub fn new(
        file_path: PathBuf,
        original_range: (usize, usize),
        replacement_text: String,
        config: ReplacementConfig,
    ) -> Self {
        Self {
            file_path,
            original_range,
            replacement_text,
            config,
        }
    }
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
        write!(f, "{}", self.description)
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
        let original_metadata = std::fs::metadata(&file_path).ok();
        Ok(Self {
            file_path,
            replacements: Vec::new(),
            original_metadata,
        })
    }

    /// Adds a replacement task to this plan
    pub fn add_replacement(&mut self, task: ReplacementTask) {
        self.replacements.push(task);
        // Keep replacements sorted by range start for efficient application
        self.replacements.sort_by_key(|r| r.original_range.0);
    }

    /// Applies the replacements to the file using the appropriate strategy
    fn apply(
        &self,
        config: &ReplacementConfig,
        metrics: &MemoryMetrics,
    ) -> SearchResult<Option<PathBuf>> {
        let metadata = fs::metadata(&self.file_path).map_err(SearchError::IoError)?;

        let strategy = ProcessingStrategy::for_file_size(metadata.len());

        match strategy {
            ProcessingStrategy::InMemory => self.apply_in_memory(config, metrics),
            ProcessingStrategy::Streaming => self.apply_streaming(config, metrics),
            ProcessingStrategy::MemoryMapped => self.apply_memory_mapped(config, metrics),
        }
    }

    /// Process small files entirely in memory
    fn apply_in_memory(
        &self,
        config: &ReplacementConfig,
        metrics: &MemoryMetrics,
    ) -> SearchResult<Option<PathBuf>> {
        // Create backup if enabled
        let backup_path = if config.backup_enabled && !config.dry_run {
            self.create_backup(config)?
        } else {
            None
        };

        if config.dry_run {
            return Ok(None);
        }

        // Read the entire file content as bytes
        let content = fs::read(&self.file_path).map_err(SearchError::IoError)?;
        metrics.record_allocation(content.len() as u64);

        // Apply replacements in reverse order
        let mut new_content = content;
        for task in self.replacements.iter().rev() {
            let replacement_bytes = task.replacement_text.as_bytes();
            let start = task.original_range.0;
            let end = task.original_range.1;

            // Create new buffer with the replacement
            let mut result =
                Vec::with_capacity(new_content.len() - (end - start) + replacement_bytes.len());
            result.extend_from_slice(&new_content[..start]);
            result.extend_from_slice(replacement_bytes);
            result.extend_from_slice(&new_content[end..]);
            new_content = result;
        }

        // Write modified content
        fs::write(&self.file_path, new_content).map_err(SearchError::IoError)?;

        if config.preserve_metadata {
            if let Some(metadata) = &self.original_metadata {
                fs::set_permissions(&self.file_path, metadata.permissions()).ok();
            }
        }

        Ok(backup_path)
    }

    /// Process medium files using buffered streaming I/O
    fn apply_streaming(
        &self,
        config: &ReplacementConfig,
        _metrics: &MemoryMetrics,
    ) -> SearchResult<Option<PathBuf>> {
        // Create backup if enabled
        let backup_path = if config.backup_enabled && !config.dry_run {
            self.create_backup(config)?
        } else {
            None
        };

        if config.dry_run {
            return Ok(None);
        }

        let input_file = File::open(&self.file_path).map_err(SearchError::IoError)?;
        let mut reader = BufReader::new(input_file);

        let tmp_path = self.file_path.with_extension("tmp");
        let output_file = File::create(&tmp_path).map_err(SearchError::IoError)?;
        let mut writer = BufWriter::new(output_file);

        let mut current_offset = 0u64;
        let mut buffer = [0u8; 8192];

        for task in &self.replacements {
            let start = task.original_range.0 as u64;
            let end = task.original_range.1 as u64;

            // Copy bytes from current_offset to start
            while current_offset < start {
                let to_read = std::cmp::min(start - current_offset, buffer.len() as u64) as usize;

                let bytes_read = reader
                    .read(&mut buffer[..to_read])
                    .map_err(SearchError::IoError)?;
                if bytes_read == 0 {
                    break;
                }

                writer
                    .write_all(&buffer[..bytes_read])
                    .map_err(SearchError::IoError)?;
                current_offset += bytes_read as u64;
            }

            // Write replacement text
            writer
                .write_all(task.replacement_text.as_bytes())
                .map_err(SearchError::IoError)?;

            // Skip replaced content in input
            reader
                .seek(SeekFrom::Current((end - start) as i64))
                .map_err(SearchError::IoError)?;
            current_offset = end;
        }

        // Copy remaining content
        loop {
            let bytes_read = reader.read(&mut buffer).map_err(SearchError::IoError)?;
            if bytes_read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..bytes_read])
                .map_err(SearchError::IoError)?;
        }

        writer.flush().map_err(SearchError::IoError)?;
        drop(writer);

        fs::rename(&tmp_path, &self.file_path).map_err(SearchError::IoError)?;

        if config.preserve_metadata {
            if let Some(metadata) = &self.original_metadata {
                fs::set_permissions(&self.file_path, metadata.permissions()).ok();
            }
        }

        Ok(backup_path)
    }

    /// Process large files using memory mapping
    fn apply_memory_mapped(
        &self,
        config: &ReplacementConfig,
        _metrics: &MemoryMetrics,
    ) -> SearchResult<Option<PathBuf>> {
        // Create backup if enabled
        let backup_path = if config.backup_enabled && !config.dry_run {
            self.create_backup(config)?
        } else {
            None
        };

        if config.dry_run {
            return Ok(None);
        }

        let file = File::open(&self.file_path).map_err(SearchError::IoError)?;
        let mmap = unsafe { MmapOptions::new().map(&file) }.map_err(SearchError::IoError)?;

        let tmp_path = self.file_path.with_extension("tmp");
        let output_file = File::create(&tmp_path).map_err(SearchError::IoError)?;
        let mut writer = BufWriter::new(output_file);

        let mut current_offset = 0;

        for task in &self.replacements {
            let start = task.original_range.0;

            // Write unmodified content
            writer
                .write_all(&mmap[current_offset..start])
                .map_err(SearchError::IoError)?;

            // Write replacement
            writer
                .write_all(task.replacement_text.as_bytes())
                .map_err(SearchError::IoError)?;

            current_offset = task.original_range.1;
        }

        // Write remaining content
        if current_offset < mmap.len() {
            writer
                .write_all(&mmap[current_offset..])
                .map_err(SearchError::IoError)?;
        }

        writer.flush().map_err(SearchError::IoError)?;
        drop(writer);

        fs::rename(&tmp_path, &self.file_path).map_err(SearchError::IoError)?;

        if config.preserve_metadata {
            if let Some(metadata) = &self.original_metadata {
                fs::set_permissions(&self.file_path, metadata.permissions()).ok();
            }
        }

        Ok(backup_path)
    }

    /// Create a backup of the file
    fn create_backup(&self, config: &ReplacementConfig) -> SearchResult<Option<PathBuf>> {
        if !config.backup_enabled || config.dry_run {
            return Ok(None);
        }

        let backup_dir = config.backup_dir.clone().unwrap_or_else(|| {
            let backups = config.undo_dir.join("backups");
            fs::create_dir_all(&backups)
                .map_err(|e| {
                    SearchError::config_error(format!("Failed to create backup directory: {}", e))
                })
                .unwrap();
            backups
        });

        let file_path_abs = self.file_path.canonicalize().map_err(|e| {
            SearchError::config_error(format!("Failed to get absolute path: {}", e))
        })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let backup_path = backup_dir.join(format!(
            "{}.{}",
            file_path_abs.file_name().unwrap().to_string_lossy(),
            timestamp
        ));

        fs::copy(&file_path_abs, &backup_path).map_err(SearchError::IoError)?;
        Ok(Some(backup_path))
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
                Ok(())
            })?;

        let backups = backups.into_inner().unwrap();

        // Save undo information
        if !self.config.dry_run && !backups.is_empty() {
            self.save_undo_info(&backups)?;
        }

        Ok(())
    }

    /// Generates a preview of the changes in parallel
    pub fn preview(&self) -> SearchResult<Vec<PreviewResult>> {
        let mut results = Vec::new();
        for plan in &self.plans {
            let content = fs::read_to_string(&plan.file_path).map_err(SearchError::IoError)?;
            let mut original_lines = Vec::new();
            let mut new_lines = Vec::new();
            let mut line_numbers = Vec::new();

            // Split content into lines and track line numbers
            let lines: Vec<_> = content.lines().enumerate().collect();
            let mut current_pos = 0;

            for (line_number, line) in lines {
                let line_start = current_pos;
                let line_end = line_start + line.len();

                // Check if this line contains any replacements
                let mut line_modified = false;
                let mut line_content = line.to_string();

                for task in &plan.replacements {
                    let range = task.original_range;
                    if range.0 >= line_start && range.0 < line_end {
                        if !line_modified {
                            original_lines.push(line.to_string());
                            line_numbers.push(line_number + 1);
                            line_modified = true;
                        }

                        // Apply replacement to this line
                        let local_start = range.0 - line_start;
                        let local_end = range.1.min(line_end) - line_start;
                        line_content.replace_range(local_start..local_end, &task.replacement_text);
                    }
                }

                if line_modified {
                    new_lines.push(line_content);
                }

                current_pos = line_end + 1; // +1 for newline
            }

            if !original_lines.is_empty() {
                results.push(PreviewResult {
                    file_path: plan.file_path.clone(),
                    original_lines,
                    new_lines,
                    line_numbers,
                });
            }
        }
        Ok(results)
    }

    /// Save undo information for a set of backups
    fn save_undo_info(&self, backups: &[(PathBuf, PathBuf)]) -> SearchResult<()> {
        let undo_dir = &self.config.undo_dir;
        fs::create_dir_all(undo_dir).map_err(|e| {
            SearchError::config_error(format!("Failed to create undo directory: {}", e))
        })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let info = UndoInfo {
            timestamp,
            description: format!(
                "Replace '{}' with '{}'",
                self.config.pattern, self.config.replacement
            ),
            backups: backups.to_vec(),
            total_size: backups
                .iter()
                .map(|(_, b)| fs::metadata(b).map(|m| m.len()).unwrap_or(0))
                .sum(),
            file_count: backups.len(),
            dry_run: self.config.dry_run,
        };

        let info_path = undo_dir.join(format!("{}.json", timestamp));
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

impl ReplacementTask {
    pub fn apply(&self, content: &str) -> Result<String, SearchError> {
        if self.config.is_regex {
            let regex = regex::Regex::new(&self.config.pattern)
                .map_err(|e| SearchError::invalid_pattern(e.to_string()))?;

            if let Some(capture_fmt) = &self.config.capture_groups {
                Ok(regex.replace_all(content, capture_fmt).into_owned())
            } else {
                Ok(regex
                    .replace_all(content, &self.config.replacement)
                    .into_owned())
            }
        } else {
            Ok(content.replace(&self.config.pattern, &self.config.replacement))
        }
    }
}
