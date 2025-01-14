# Introducing Search and Replace in RustScout

We're excited to announce a powerful new feature in RustScout: search and replace functionality. This addition transforms RustScout from a high-performance code search tool into a comprehensive codebase refactoring solution, while maintaining its core principles of performance, safety, and reliability.

## Key Features

1. **Intelligent File Processing**
   - Small files (<32KB): Direct in-memory operations
   - Medium files (32KB-10MB): Buffered streaming
   - Large files (>10MB): Memory-mapped access
   - O(1) memory usage regardless of file size

2. **Safety First**
   - Atomic file updates using temporary files
   - Optional file backups
   - Undo support with detailed operation tracking
   - Preservation of file permissions and timestamps

3. **Rich Preview Options**
   - Dry-run mode to see changes without applying them
   - Detailed previews showing before/after content
   - Line-by-line change visualization
   - Context-aware replacement preview

4. **Flexible Pattern Matching**
   - Simple text replacement
   - Regular expressions with capture groups
   - Multiple replacements per file
   - Pattern validation and optimization

## Implementation Details

### Core Data Structures

```rust
pub struct ReplacementTask {
    pub file_path: PathBuf,
    pub original_range: (usize, usize),
    pub replacement_text: String,
    pub config: ReplacementConfig,
}

pub struct FileReplacementPlan {
    pub file_path: PathBuf,
    pub replacements: Vec<ReplacementTask>,
    pub original_metadata: Option<std::fs::Metadata>,
}

pub struct ReplacementSet {
    pub config: ReplacementConfig,
    pub plans: Vec<FileReplacementPlan>,
    metrics: Arc<MemoryMetrics>,
}

pub struct UndoInfo {
    pub timestamp: u64,
    pub description: String,
    pub backups: Vec<(PathBuf, PathBuf)>,
    pub total_size: u64,
    pub file_count: usize,
    pub dry_run: bool,
}
```

### Processing Strategies

1. **Small File Processing**
```rust
fn apply_in_memory(&self, config: &ReplacementConfig) -> SearchResult<Option<PathBuf>> {
    let content = fs::read_to_string(&self.file_path)?;
    let mut result = content.clone();
    
    // Apply replacements in reverse order to maintain correct offsets
    for task in self.replacements.iter().rev() {
        result.replace_range(
            task.original_range.0..task.original_range.1,
            &task.replacement_text
        );
    }
    
    // Write to temporary file and rename atomically
    let tmp_path = create_temp_file(&result)?;
    fs::rename(&tmp_path, &self.file_path)?;
    Ok(None)
}
```

2. **Buffered Processing**
```rust
fn apply_streaming(&self, config: &ReplacementConfig) -> SearchResult<Option<PathBuf>> {
    let mut reader = BufReader::new(File::open(&self.file_path)?);
    let tmp_path = self.file_path.with_extension("tmp");
    let mut writer = BufWriter::new(File::create(&tmp_path)?);
    
    let mut current_pos = 0;
    for task in &self.replacements {
        // Copy unchanged content
        io::copy(
            &mut reader.take(task.original_range.0 as u64 - current_pos),
            &mut writer
        )?;
        
        // Write replacement
        writer.write_all(task.replacement_text.as_bytes())?;
        reader.seek(SeekFrom::Start(task.original_range.1 as u64))?;
        current_pos = task.original_range.1 as u64;
    }
    
    // Copy remaining content
    io::copy(&mut reader, &mut writer)?;
    fs::rename(&tmp_path, &self.file_path)?;
    Ok(None)
}
```

3. **Memory-Mapped Processing**
```rust
fn apply_memory_mapped(&self, config: &ReplacementConfig) -> SearchResult<Option<PathBuf>> {
    let file = File::open(&self.file_path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let tmp_path = self.file_path.with_extension("tmp");
    let mut writer = BufWriter::new(File::create(&tmp_path)?);
    
    let mut current_pos = 0;
    for task in &self.replacements {
        writer.write_all(&mmap[current_pos..task.original_range.0])?;
        writer.write_all(task.replacement_text.as_bytes())?;
        current_pos = task.original_range.1;
    }
    
    writer.write_all(&mmap[current_pos..])?;
    fs::rename(&tmp_path, &self.file_path)?;
    Ok(None)
}
```

### Backup and Undo Support

```rust
impl ReplacementSet {
    pub fn list_undo_operations() -> SearchResult<Vec<(UndoInfo, PathBuf)>> {
        // List available undo operations from the undo directory
    }

    pub fn undo_by_id(id: usize) -> SearchResult<()> {
        // Restore files from backups for the specified operation
    }

    fn save_undo_info(&self, backups: &[(PathBuf, PathBuf)]) -> SearchResult<()> {
        // Save metadata about the operation for future undo
    }
}
```

## Usage Examples

### Simple Text Replacement
```bash
# Replace "foo" with "bar" in all Rust files
rustscout replace "foo" --replace "bar" src/*.rs

# Preview changes first
rustscout replace "TODO" --replace "DONE" --dry-run src/
```

### Regex Replacement
```bash
# Rename functions to add a prefix
rustscout replace --regex 'fn\s+(\w+)' --capture-groups 'fn new_$1' src/

# With backup and undo support
rustscout replace "old_api" --replace "new_api" --backup src/
rustscout replace --undo  # Reverts the last change
```

### Advanced Options
```bash
# Preserve file metadata
rustscout replace "pattern" --replace "new" --preserve src/

# Custom backup directory
rustscout replace "pattern" --replace "new" --backup --output-dir backups/ src/

# Show detailed preview
rustscout replace "pattern" --replace "new" --preview src/
```

## Performance Benchmarks

Our benchmarks show excellent performance across different scenarios:

1. **Small File Replacement**
   - Simple pattern: ~88µs median time
   - Regex pattern: ~120µs median time

2. **Medium File (1MB)**
   - Single replacement: ~2.1ms
   - Multiple replacements: ~2.3ms

3. **Large File (10MB)**
   - Memory-mapped: ~52ms
   - With backup: ~104ms

4. **Batch Processing**
   - 100 small files: ~15ms
   - Mixed sizes: ~45ms
   - With previews: ~12ms

## Future Enhancements

We're planning several improvements:
1. Interactive mode for confirming changes
2. Directory-wide undo operations
3. Integration with version control systems
4. Structured code refactoring support
5. Language-aware replacements

## Contributing

We welcome contributions! Whether it's:
- Performance improvements
- Feature suggestions
- Bug reports
- Documentation enhancements

Visit our [GitHub repository](https://github.com/willibrandon/rustscout) to get involved.

## Acknowledgments

Special thanks to:
- The Rust community for excellent tools and crates
- Our users for valuable feedback and feature requests
- Contributors who helped implement and test the replacement feature

---

*This post is part of our series on building high-performance developer tools in Rust. Follow us for more updates!* 