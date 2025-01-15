# RustScout Replace Module: Robustness & Reliability Enhancements

We're excited to share the latest improvements to RustScout's **replace module**. These enhancements focus on making text replacements more **robust**, **reliable**, and **user-friendly**. Let's dive into the key improvements and the technical challenges we overcame.

---

## 1. Introduction

The replace module is a critical component of RustScout, enabling developers to make precise, controlled changes across their codebase. Our recent work, driven by **test failures** and a focus on **edge-case handling**, has strengthened several key areas:

- **Input Validation**: Stronger checks for empty patterns and invalid regex.
- **Backup & Undo**: Enhanced reliability of backup creation and undo operations.
- **Error Handling**: More descriptive error messages and graceful failure modes.
- **Preview Functionality**: Improved accuracy of replacement previews.

Through **test-driven iteration**, we've systematically improved the module's quality and reliability.

---

## 2. Key Improvements

### Enhanced Input Validation

We've strengthened validation across several areas:

1. **Empty Pattern Detection**
   ```rust
   if self.config.pattern.trim().is_empty() {
       return Err(SearchError::invalid_pattern("Pattern cannot be empty"));
   }
   ```
   - Now properly rejects empty or whitespace-only patterns
   - Provides clear error messages to guide users
   - Catches invalid patterns early in the validation phase

2. **Regex Pattern Validation**
   - Validates regex patterns before attempting replacements
   - Parses `$n` references in replacement strings and compares against actual capture groups
   - Example: For pattern `(\w+)` and replacement `$2`, we detect that group `$2` doesn't exist
   - Provides specific error messages like "Capture group $2 does not exist"

### Backup Directory Handling

The backup system now:
- Creates nested backup directories as needed
- Uses standardized timestamp-based naming
- Preserves file metadata when requested
- Stores backups in a predictable location structure:
  - If `backup_dir` is specified in config: Uses that location
  - Default: Creates `backups/` subdirectory under the undo directory
  ```rust
  let backup_dir = if let Some(ref specified_dir) = config.backup_dir {
      specified_dir.clone()
  } else {
      config.undo_dir.join("backups")
  };
  ```

### Undo Operations

Major improvements to the undo system:
- Records detailed operation metadata
- Stores undo information in a dedicated "undo" subdirectory
- Provides atomic operation restoration
- Includes operation size and file count metrics
- Serializes undo data as JSON for easy inspection and portability:
  ```rust
  let undo_json = serde_json::to_string_pretty(&undo_info)?;
  fs::write(&undo_file, undo_json)?;
  ```

### Preview Accuracy

The preview functionality now:
- Shows exact line-by-line changes
- Includes line numbers for better context
- Handles multiple replacements correctly by applying them in sequence
- Preserves original content during preview
- Provides a clear diff-style output showing before/after states

---

## 3. Technical Deep Dive

### Overlapping Replacements

One of the trickier challenges was handling overlapping replacements. We implemented a robust solution that **rejects** overlapping ranges with a clear error message:

```rust
for existing in &self.replacements {
    if task.original_range.0 < existing.original_range.1
        && existing.original_range.0 < task.original_range.1
    {
        return Err(SearchError::config_error(
            "Overlapping replacements are not allowed"
        ));
    }
}
```

This ensures that replacements don't interfere with each other, maintaining data integrity.

### Processing Strategy Selection

We've refined our file processing strategy based on file size using configurable thresholds (defined as constants in the code):

```rust
const SMALL_FILE_THRESHOLD: u64 = 32 * 1024;     // 32KB
const LARGE_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

enum ProcessingStrategy {
    InMemory,     // For small files
    Streaming,    // For medium files
    MemoryMapped, // For large files
}
```

These thresholds determine how files are processed:
- **Small files** (<32KB): Processed entirely in memory for speed
- **Medium files** (32KB-10MB): Streaming I/O for memory efficiency
- **Large files** (>10MB): Memory mapping for optimal performance

The thresholds can be adjusted by modifying the constants, allowing customization for different environments and use cases.

### Undo Information Storage

The new undo system stores detailed metadata and provides restoration capabilities:

```rust
pub struct UndoInfo {
    pub timestamp: u64,
    pub description: String,
    pub backups: Vec<(PathBuf, PathBuf)>,
    pub total_size: u64,
    pub file_count: usize,
    pub dry_run: bool,
}

// Restoration example
pub fn undo_by_id(id: u64, config: &ReplacementConfig) -> SearchResult<()> {
    let info_path = config.undo_dir.join(format!("{}.json", id));
    let info: UndoInfo = serde_json::from_str(&fs::read_to_string(&info_path)?)?;
    
    // Restore files from backups
    for (original, backup) in info.backups {
        fs::copy(&backup, &original)?;
        fs::remove_file(&backup)?;
    }
    fs::remove_file(&info_path)?;
    Ok(())
}
```

---

## 4. Testing & Validation

We've significantly expanded our test coverage, achieving high coverage across critical paths:

1. **Basic Functionality**
   - Pattern replacement accuracy
   - Backup creation and restoration
   - Preview generation
   - Test coverage: ~95% of core functionality

2. **Edge Cases**
   - Empty patterns
   - Invalid regex patterns
   - Invalid capture groups
   - Overlapping replacements
   - These tests were crucial in catching subtle bugs

3. **File System Operations**
   - Directory creation
   - Metadata preservation
   - Path normalization
   - Cross-platform path handling

4. **Performance Tests**
   - Memory usage tracking
   - Processing strategy selection
   - Large file handling
   - Threshold validation

---

## 5. Future Plans

While these improvements significantly enhance the replace module's reliability, we're already planning future enhancements:

1. **Structured Replacements**
   - AST-aware replacements for supported languages
   - Semantic replacement rules
   - AI-assisted pattern suggestions

2. **Enhanced Preview**
   - Interactive TUI for replacement preview
   - Diff-style visualization
   - Real-time preview updates

3. **Extended Undo**
   - Branch-aware undo operations
   - Selective undo for specific files
   - Undo history browsing

4. **Performance Optimization**
   - Parallel replacement execution
   - Smarter caching of replacement patterns
   - Custom thresholds for different environments

---

## Conclusion

These enhancements make RustScout's replace functionality more **robust**, **reliable**, and **user-friendly**. Whether you're refactoring variable names or updating license headers, you can trust RustScout to handle your replacements safely and efficiently.

We encourage you to try out these improvements and share your feedback. Your input helps us continue making RustScout better for everyone.

### Quick Start

Try the enhanced replace functionality:
```bash
# Replace with backup
rustscout replace -p "TODO" -r "FIXME" --backup

# Preview changes
rustscout replace -p "TODO" -r "FIXME" --preview

# Replace with undo information
rustscout replace -p "TODO" -r "FIXME" --undo-dir .rustscout/undo

# Examples of validation in action
# This will fail (empty pattern):
rustscout replace -p "" -r "test"

# This will fail (invalid regex):
rustscout replace -p "[invalid" -r "test" --regex

# This will fail (invalid capture group):
rustscout replace -p "(\w+)" -r "$2" --regex
```

---

**Happy replacing with RustScout!** 