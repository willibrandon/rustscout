use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use indicatif::{ProgressBar, ProgressStyle};
use memmap2::MmapOptions;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

use crate::errors::{SearchError, SearchResult};
use crate::metrics::MemoryMetrics;
use crate::search::matcher::{PatternDefinition, WordBoundaryMode};
use crate::workspace::detect_workspace_root;

mod undo_info;
pub use undo_info::{DiffHunk, FileDiff, UndoFileReference, UndoInfo};

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

impl Default for ReplacementConfig {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            backup_enabled: true,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: true,
            undo_dir: PathBuf::from(".rustscout/undo"),
        }
    }
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
        // Don't create backups or modify files in dry run mode
        if config.dry_run {
            return Ok(None);
        }

        // Create backup if enabled
        let backup_path = if config.backup_enabled {
            self.create_backup(config)?
        } else {
            None
        };

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

    /// Create a backup of the file if backup is enabled
    fn create_backup(&self, config: &ReplacementConfig) -> SearchResult<Option<PathBuf>> {
        if !config.backup_enabled {
            println!("Debug: Backup not enabled");
            return Ok(None);
        }

        // 1) Figure out the workspace root
        let workspace_root = detect_workspace_root(&self.file_path)?;
        println!("Debug: Workspace root = {}", workspace_root.display());

        // 2) Determine the "backups" subdirectory
        let backup_dir = if let Some(ref dir) = config.backup_dir {
            println!("Debug: Using user-specified backup dir: {}", dir.display());
            dir.clone()
        } else {
            let default_dir = workspace_root.join(".rustscout").join("backups");
            println!("Debug: Using default backup dir: {}", default_dir.display());
            default_dir
        };
        println!("Debug: Creating backup dir: {}", backup_dir.display());
        fs::create_dir_all(&backup_dir)?;

        // 3) Compute a unique backup filename from the *relative path*
        let relative = self
            .file_path
            .strip_prefix(&workspace_root)
            .unwrap_or(&self.file_path);
        println!("Debug: Relative path for backup: {}", relative.display());
        let relative_str = relative
            .to_string_lossy()
            .replace("\\", "_")
            .replace("/", "_");
        println!("Debug: Sanitized relative path: {}", relative_str);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 4) Build the final backup filename (use path-based name + timestamp)
        // e.g. "crate_a_lib.rs.1737267859"
        let backup_name = format!("{}.{}", relative_str, timestamp);
        let backup_path = backup_dir.join(&backup_name);
        println!("Debug: Final backup path: {}", backup_path.display());

        // 5) Copy original file to the new backup path
        println!(
            "Debug: Copying from {} to {}",
            self.file_path.display(),
            backup_path.display()
        );
        match fs::copy(&self.file_path, &backup_path) {
            Ok(_) => println!("Debug: Successfully created backup"),
            Err(e) => println!("Debug: Failed to create backup: {}", e),
        }

        if config.preserve_metadata {
            if let Ok(metadata) = fs::metadata(&self.file_path) {
                let _ = fs::set_permissions(&backup_path, metadata.permissions());
            }
        }

        Ok(Some(backup_path))
    }

    /// Generates a preview of the changes
    pub fn preview(&self) -> SearchResult<Vec<PreviewResult>> {
        let mut results = Vec::new();

        // Get the content
        let content = fs::read_to_string(&self.file_path).map_err(SearchError::IoError)?;
        let mut new_content = content.clone();

        // Apply replacements in reverse order to maintain correct offsets
        for task in self.replacements.iter().rev() {
            new_content.replace_range(
                task.original_range.0..task.original_range.1,
                &task.replacement_text,
            );
        }

        // Compare line by line
        let original_lines: Vec<&str> = content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut changed_original = Vec::new();
        let mut changed_new = Vec::new();
        let mut line_numbers = Vec::new();

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

    /// Returns the old and new content for this file
    pub fn preview_old_new(&self) -> SearchResult<(String, String)> {
        let content = fs::read_to_string(&self.file_path)?;
        let mut new_content = content.clone();

        // Apply all replacements
        for task in &self.replacements {
            new_content = task.apply(&new_content)?;
        }

        Ok((content, new_content))
    }

    /// Reconstruct the original file from the new content by reversing each hunk.
    pub fn revert_file_with_hunks(file_diff: &FileDiff) -> Result<(), SearchError> {
        let path = &file_diff.file_path;
        if !path.exists() {
            return Err(SearchError::config_error(format!(
                "Cannot revert. File '{}' no longer exists.",
                path.display()
            )));
        }

        // Read current file content
        let new_content = std::fs::read_to_string(path).map_err(SearchError::IoError)?;

        let mut lines: Vec<String> = new_content.lines().map(|l| l.to_string()).collect();

        // Sort hunks in descending order of new_start_line so we can safely patch from bottom to top
        let mut hunks = file_diff.hunks.clone();
        hunks.sort_by_key(|h| std::cmp::Reverse(h.new_start_line));

        for hunk in hunks {
            let new_start = hunk.new_start_line.saturating_sub(1);
            // Remove the lines that were "newly added" in that region
            if hunk.new_line_count > 0 {
                let end = new_start + hunk.new_line_count.min(lines.len() - new_start);
                lines.drain(new_start..end);
            }
            // Then re-insert the old lines
            if !hunk.original_lines.is_empty() {
                for (i, old_line) in hunk.original_lines.iter().enumerate() {
                    lines.insert(new_start + i, old_line.clone());
                }
            }
        }

        // Join lines back and overwrite file
        let reverted_content = lines.join("\n");
        std::fs::write(path, reverted_content).map_err(SearchError::IoError)?;

        Ok(())
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

    /// Lists available undo operations with detailed information about each change
    pub fn list_undo_operations_verbose(config: &ReplacementConfig) -> SearchResult<Vec<UndoInfo>> {
        let operations = Self::list_undo_operations(config)?;

        for (info, _path) in &operations {
            println!("ID: {}  =>  {}", info.timestamp, info.description);

            if !info.file_diffs.is_empty() {
                for (file_idx, fd) in info.file_diffs.iter().enumerate() {
                    println!(
                        "  File #{}: {}",
                        file_idx + 1,
                        fd.file_path.rel_path.display()
                    );
                    for (hunk_idx, h) in fd.hunks.iter().enumerate() {
                        println!(
                            "    Hunk {}: lines {}-{} replaced with lines {}-{}",
                            hunk_idx + 1,
                            h.original_start_line,
                            h.original_start_line + h.original_line_count - 1,
                            h.new_start_line,
                            h.new_start_line + h.new_line_count - 1
                        );
                    }
                }
            } else {
                for (original, backup) in &info.backups {
                    println!(
                        "  File: {} -> {}",
                        original.rel_path.display(),
                        backup.rel_path.display()
                    );
                }
            }
        }

        Ok(operations.into_iter().map(|(info, _)| info).collect())
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
            self.save_undo_info(&backup_paths)?;
        }

        Ok(())
    }

    /// Generates a preview of the changes in parallel
    pub fn preview(&self) -> SearchResult<Vec<PreviewResult>> {
        let mut results = Vec::new();

        for plan in &self.plans {
            let mut plan_results = plan.preview()?;
            results.append(&mut plan_results);
        }

        Ok(results)
    }

    /// Save undo information for this replacement operation
    fn save_undo_info(&self, backups: &[(PathBuf, PathBuf)]) -> SearchResult<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Convert paths to UndoFileReferences
        let mut file_refs = Vec::new();
        for (original, backup) in backups {
            let original_ref = UndoFileReference::new(original)?;
            let backup_ref = UndoFileReference::new(backup)?;
            file_refs.push((original_ref, backup_ref));
        }

        // Create file diffs
        let mut file_diffs = Vec::new();
        for plan in &self.plans {
            if let Ok((old_content, new_content)) = plan.preview_old_new() {
                let file_ref = UndoFileReference::new(&plan.file_path)?;
                let diff = generate_file_diff(&old_content, &new_content, &plan.file_path);
                file_diffs.push(FileDiff {
                    file_path: file_ref,
                    hunks: diff.hunks,
                });
            }
        }

        // Create a descriptive message about the replacements
        let description = if !self.config.patterns.is_empty() {
            let pattern = &self.config.patterns[0];
            format!(
                "Replace '{}' with '{}'",
                pattern.definition.text, pattern.replacement_text
            )
        } else {
            format!("Replacement operation at {}", timestamp)
        };

        let info = UndoInfo {
            timestamp,
            description,
            backups: file_refs,
            total_size: backups
                .iter()
                .map(|(_, b)| fs::metadata(b).map(|m| m.len()).unwrap_or(0))
                .sum(),
            file_count: backups.len(),
            dry_run: self.config.dry_run,
            file_diffs,
        };

        let undo_dir = self.config.undo_dir.clone();
        fs::create_dir_all(&undo_dir).map_err(SearchError::IoError)?;

        let undo_file = undo_dir.join(format!("{}.json", timestamp));
        let content = serde_json::to_string_pretty(&info).map_err(SearchError::JsonError)?;
        fs::write(&undo_file, content).map_err(SearchError::IoError)?;

        Ok(())
    }

    /// Undoes a specific operation by its ID
    pub fn undo_by_id(id: u64, config: &ReplacementConfig) -> SearchResult<()> {
        let info_path = config.undo_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&info_path)
            .map_err(|e| SearchError::config_error(format!("Failed to read undo info: {}", e)))?;
        let info: UndoInfo = serde_json::from_str(&content)?;

        // Detect workspace root from the undo directory which we know exists
        let workspace_root = detect_workspace_root(&config.undo_dir)?;
        println!("Debug: undo workspace_root = {}", workspace_root.display());

        // Restore files from backups
        for (original, backup) in &info.backups {
            let path_to_restore = if let Some(abs) = original.abs_path.as_ref() {
                if abs.exists() {
                    println!("Debug: Using absolute path for restore: {}", abs.display());
                    abs.clone()
                } else {
                    let fallback = workspace_root.join(&original.rel_path);
                    println!(
                        "Debug: Using fallback path for restore: {}",
                        fallback.display()
                    );
                    fallback
                }
            } else {
                let fallback = workspace_root.join(&original.rel_path);
                println!(
                    "Debug: Using relative path for restore: {}",
                    fallback.display()
                );
                fallback
            };

            let backup_path = if let Some(abs) = backup.abs_path.as_ref() {
                if abs.exists() {
                    println!("Debug: Using absolute backup path: {}", abs.display());
                    abs.clone()
                } else {
                    let fallback = workspace_root.join(&backup.rel_path);
                    println!("Debug: Using fallback backup path: {}", fallback.display());
                    fallback
                }
            } else {
                let fallback = workspace_root.join(&backup.rel_path);
                println!("Debug: Using relative backup path: {}", fallback.display());
                fallback
            };

            // Ensure backup exists and has content
            if !backup_path.exists() {
                return Err(SearchError::config_error(format!(
                    "Backup file not found: {}",
                    backup_path.display()
                )));
            }

            // Read backup content and write to original file
            let backup_content = fs::read_to_string(&backup_path)
                .map_err(|e| SearchError::config_error(format!("Failed to read backup: {}", e)))?;

            println!(
                "Debug: Writing backup content to: {}",
                path_to_restore.display()
            );
            fs::write(&path_to_restore, backup_content).map_err(|e| {
                SearchError::config_error(format!("Failed to restore backup: {}", e))
            })?;

            // Clean up backup file
            fs::remove_file(&backup_path).ok();
        }

        // Clean up the undo info file
        fs::remove_file(info_path).ok();

        Ok(())
    }

    /// Partially reverts an existing replacement operation by only reverting selected hunk indices.
    /// If the operation has no patch-based diffs (file_diffs), returns an error.
    pub fn undo_partial_by_id(
        id: u64,
        config: &ReplacementConfig,
        hunk_indices: &[usize],
    ) -> SearchResult<()> {
        let info_path = config.undo_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&info_path)
            .map_err(|e| SearchError::config_error(format!("Failed to read undo info: {}", e)))?;
        let info: UndoInfo = serde_json::from_str(&content)?;

        // If there's no diff data, partial revert isn't possible
        if info.file_diffs.is_empty() {
            return Err(SearchError::config_error(
                "This undo operation only supports full-file backups; partial revert is not possible.",
            ));
        }

        // Process each file diff
        for file_diff in &info.file_diffs {
            let workspace_root = detect_workspace_root(&file_diff.file_path.rel_path)?;
            let path_to_restore = if let Some(abs) = file_diff.file_path.abs_path.as_ref() {
                if abs.exists() {
                    abs.clone()
                } else {
                    // Fallback to workspace-relative path
                    workspace_root.join(&file_diff.file_path.rel_path)
                }
            } else {
                workspace_root.join(&file_diff.file_path.rel_path)
            };

            // Create a new file diff with only the selected hunks
            let mut filtered_diff = file_diff.clone();
            filtered_diff.hunks = file_diff
                .hunks
                .iter()
                .enumerate()
                .filter(|(i, _)| hunk_indices.contains(i))
                .map(|(_, h)| h.clone())
                .collect();

            // Apply the filtered hunks
            apply_file_diff(&path_to_restore, &filtered_diff)?;
        }

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

/// Generate a line-based diff between old and new content
pub fn generate_file_diff(old_content: &str, new_content: &str, file_path: &Path) -> FileDiff {
    let file_ref = UndoFileReference::new(file_path).unwrap_or_else(|_| UndoFileReference {
        rel_path: file_path.to_path_buf(),
        abs_path: None,
    });

    // Normalize line endings to LF
    let old_content = old_content.replace("\r\n", "\n");
    let new_content = new_content.replace("\r\n", "\n");

    let diff = TextDiff::from_lines(&old_content, &new_content);
    let mut hunks = Vec::new();

    for group in diff.grouped_ops(3) {
        for op in group {
            match op {
                similar::DiffOp::Equal { .. } => {
                    // no changes; skip
                }
                similar::DiffOp::Insert {
                    new_index,
                    new_len,
                    old_index: _,
                } => {
                    // lines added
                    let mut new_lines = Vec::new();
                    for change in diff.iter_changes(&op) {
                        if change.tag() == ChangeTag::Insert {
                            new_lines.push(change.value().trim_end().to_string());
                        }
                    }

                    hunks.push(DiffHunk {
                        original_start_line: new_index + 1, // anchor at insertion point
                        new_start_line: new_index + 1,
                        original_line_count: 0,
                        new_line_count: new_len,
                        original_lines: vec![],
                        new_lines,
                    });
                }
                similar::DiffOp::Delete {
                    old_index,
                    old_len,
                    new_index: _,
                } => {
                    // lines removed
                    let mut original_lines = Vec::new();
                    for change in diff.iter_changes(&op) {
                        if change.tag() == ChangeTag::Delete {
                            original_lines.push(change.value().trim_end().to_string());
                        }
                    }

                    hunks.push(DiffHunk {
                        original_start_line: old_index + 1,
                        new_start_line: old_index + 1, // anchor at deletion point
                        original_line_count: old_len,
                        new_line_count: 0,
                        original_lines,
                        new_lines: vec![],
                    });
                }
                similar::DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    let mut orig_lines = Vec::new();
                    let mut new_lines = Vec::new();

                    for change in diff.iter_changes(&op) {
                        match change.tag() {
                            ChangeTag::Delete => {
                                orig_lines.push(change.value().trim_end().to_string());
                            }
                            ChangeTag::Insert => {
                                new_lines.push(change.value().trim_end().to_string());
                            }
                            ChangeTag::Equal => {}
                        }
                    }

                    hunks.push(DiffHunk {
                        original_start_line: old_index + 1,
                        new_start_line: new_index + 1,
                        original_line_count: old_len,
                        new_line_count: new_len,
                        original_lines: orig_lines,
                        new_lines,
                    });
                }
            }
        }
    }

    FileDiff {
        file_path: file_ref,
        hunks,
    }
}

/// Apply a file diff to restore a file to its previous state
fn apply_file_diff(path: &Path, file_diff: &FileDiff) -> SearchResult<()> {
    if !path.exists() {
        return Err(SearchError::config_error(format!(
            "File to revert does not exist: {}",
            path.display()
        )));
    }

    let new_content = std::fs::read_to_string(path).map_err(SearchError::IoError)?;
    let mut lines: Vec<String> = new_content.lines().map(String::from).collect();

    // Sort hunks in descending order of new_start_line so we can safely patch from bottom to top
    let mut hunks = file_diff.hunks.clone();
    hunks.sort_by_key(|h| std::cmp::Reverse(h.new_start_line));

    for hunk in hunks {
        let new_start = hunk.new_start_line.saturating_sub(1);
        // Remove the lines that were "newly added" in that region
        if hunk.new_line_count > 0 {
            let end = new_start + hunk.new_line_count.min(lines.len() - new_start);
            lines.drain(new_start..end);
        }
        // Then re-insert the old lines
        if !hunk.original_lines.is_empty() {
            for (i, old_line) in hunk.original_lines.iter().enumerate() {
                lines.insert(new_start + i, old_line.clone());
            }
        }
    }

    // Join lines back and overwrite file
    let reverted_content = lines.join("\n");
    std::fs::write(path, reverted_content).map_err(SearchError::IoError)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::init_workspace;
    use std::fs;
    use tempfile::TempDir;

    // Helper function to create a basic pattern definition
    fn create_pattern_def(text: &str, is_regex: bool) -> PatternDefinition {
        PatternDefinition {
            text: text.to_string(),
            is_regex,
            boundary_mode: WordBoundaryMode::None,
            hyphen_mode: crate::search::matcher::HyphenMode::default(),
        }
    }

    #[test]
    fn test_processing_strategies() -> SearchResult<()> {
        // Create test files with known sizes
        let dir = TempDir::new().unwrap();

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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let dir = TempDir::new().unwrap();
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
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Initialize workspace
        init_workspace(root, "json")?;

        // Create test files
        let original = root.join("test.txt");
        fs::write(&original, "original content")?;

        let backup = root
            .join(".rustscout")
            .join("backups")
            .join("test.txt.1234");
        fs::create_dir_all(backup.parent().unwrap())?;
        fs::write(&backup, "backup content")?;

        // Create undo info
        let original_ref = UndoFileReference::new(&original)?;
        let backup_ref = UndoFileReference::new(&backup)?;

        let undo_dir = root.join(".rustscout").join("undo");
        fs::create_dir_all(&undo_dir)?;

        let info = UndoInfo {
            timestamp: 1234,
            description: "Test undo".to_string(),
            backups: vec![(original_ref, backup_ref)],
            total_size: 100,
            file_count: 1,
            dry_run: false,
            file_diffs: vec![],
        };

        let undo_file = undo_dir.join("1234.json");
        let content = serde_json::to_string_pretty(&info).map_err(|e| SearchError::JsonError(e))?;
        fs::write(&undo_file, content).map_err(SearchError::IoError)?;

        // Test undo
        let config = ReplacementConfig {
            patterns: vec![],
            backup_enabled: true,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: false,
            undo_dir,
        };

        ReplacementSet::undo_by_id(1234, &config)?;

        // Verify results
        assert!(!backup.exists());
        assert_eq!(fs::read_to_string(&original)?, "backup content");
        assert!(!undo_file.exists());

        Ok(())
    }

    #[test]
    fn test_undo_info_with_diffs() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Initialize workspace
        init_workspace(root, "json")?;

        // Create test file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "line 1\nline 2\nline 3\n")?;

        let file_ref = UndoFileReference::new(&test_file)?;

        // Create diff
        let old_content = "line 1\nline 2\nline 3\n";
        let new_content = "line 1\nmodified\nline 3\n";
        let diff = generate_file_diff(old_content, new_content, &test_file);
        let diff_hunks_len = diff.hunks.len();

        // Create undo info with diff
        let info = UndoInfo {
            timestamp: 1234,
            description: "Test diff".to_string(),
            backups: vec![],
            total_size: 100,
            file_count: 1,
            dry_run: false,
            file_diffs: vec![diff],
        };

        // Verify serialization
        let json = serde_json::to_string_pretty(&info)?;
        let deserialized: UndoInfo = serde_json::from_str(&json)?;

        assert_eq!(
            deserialized.file_diffs[0].file_path.rel_path,
            file_ref.rel_path
        );
        assert_eq!(deserialized.file_diffs[0].hunks.len(), diff_hunks_len);

        Ok(())
    }

    #[test]
    fn test_undo_with_fallback() -> SearchResult<()> {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Initialize workspace
        init_workspace(root, "json")?;

        // Create test file and make a backup
        let test_file = root.join("test.txt");
        fs::write(&test_file, "original content").unwrap();

        let config = ReplacementConfig {
            patterns: vec![],
            backup_enabled: true,
            dry_run: false,
            backup_dir: None,
            preserve_metadata: true,
            undo_dir: root.join(".rustscout").join("undo"),
        };

        // Verify workspace root detection
        let workspace_root = detect_workspace_root(&config.undo_dir)?;
        println!("Debug: workspace_root = {}", workspace_root.display());
        println!("Debug: temp_dir = {}", root.display());
        assert_eq!(workspace_root, root, "Workspace root should match temp dir");

        // Create undo info with absolute path that won't exist
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let non_existent = root.join("non_existent_dir").join("test.txt");
        let original_ref = UndoFileReference {
            rel_path: PathBuf::from("test.txt"),
            abs_path: Some(non_existent.clone()),
        };

        println!("Debug: non_existent path = {}", non_existent.display());

        // Create backup directory and backup file
        fs::create_dir_all(config.undo_dir.as_path())?;
        let backup_path = config.undo_dir.join(format!("{}.bak", timestamp));
        fs::copy(&test_file, &backup_path)?;

        println!("Debug: backup_path exists = {}", backup_path.exists());
        println!(
            "Debug: backup content = {:?}",
            fs::read_to_string(&backup_path)?
        );

        let backup_ref = UndoFileReference {
            rel_path: PathBuf::from(format!(".rustscout/undo/{}.bak", timestamp)),
            abs_path: Some(backup_path.clone()),
        };

        let info = UndoInfo {
            timestamp,
            description: "Test undo".to_string(),
            backups: vec![(original_ref, backup_ref)],
            total_size: 100,
            file_count: 1,
            dry_run: false,
            file_diffs: vec![],
        };

        // Save undo info
        let undo_file = config.undo_dir.join(format!("{}.json", timestamp));
        let json = serde_json::to_string_pretty(&info)?;
        fs::write(&undo_file, json)?;

        // Modify the test file
        fs::write(&test_file, "modified content")?;

        println!("Debug: test_file exists = {}", test_file.exists());
        println!("Debug: test_file path = {}", test_file.display());
        println!(
            "Debug: test_file content = {:?}",
            fs::read_to_string(&test_file)?
        );

        // Try to undo - should fallback to relative path
        ReplacementSet::undo_by_id(timestamp, &config)?;

        // Verify content was restored
        let restored_content = fs::read_to_string(&test_file)?;
        println!("Debug: restored content = {:?}", restored_content);

        assert_eq!(restored_content, "original content");

        Ok(())
    }
}
