# RustScout

[![Crates.io](https://img.shields.io/crates/v/rustscout.svg)](https://crates.io/crates/rustscout)
[![Crates.io](https://img.shields.io/crates/v/rustscout-cli.svg)](https://crates.io/crates/rustscout-cli)
[![Documentation](https://docs.rs/rustscout/badge.svg)](https://docs.rs/rustscout)
[![License:MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://github.com/willibrandon/rustscout/workflows/CI/badge.svg)](https://github.com/willibrandon/rustscout/actions)
[![codecov](https://codecov.io/gh/willibrandon/rustscout/branch/main/graph/badge.svg)](https://codecov.io/gh/willibrandon/rustscout)

A high-performance, concurrent code search tool written in Rust. RustScout is designed for quickly searching and analyzing large codebases with a focus on performance and usability.

## Features

- 🚀 **High Performance**: Utilizes Rust's concurrency features for blazing-fast searches
- 🔍 **Incremental Search**: Smart caching for faster repeated searches
  - Automatic change detection (Git or file signatures)
  - Cache compression support
  - Configurable cache size and location
  - Intelligent cache invalidation
- 🔍 **Smart Search**: Support for multiple patterns with mix of simple text and regex
  - Word boundary matching for precise identifier search
    - Smart hyphen handling (by default, uses code/joining mode where `test-case` is one token; use `--hyphen-mode=boundary` for natural text where `hello-world` is two words)
    - Underscores always join words, even when bridging different scripts (e.g., `hello_世界` or `café_안녕`)
    - Full Unicode support for word boundaries
    - Configurable per-pattern behavior

  > **At a Glance: Word Boundary Behavior**
  > - **Hyphens**: Default (joining mode for code), or `--hyphen-mode=boundary` for text
  > - **Underscores**: Always join words (no override)
  > - **Word Boundaries**: Auto-adds `\b` unless already in pattern
  > - **Unicode**: Full support for mixed scripts and special characters

  - Mix of simple and regex patterns
  - Case-sensitive and case-insensitive modes
- 🔄 **Search and Replace**: Powerful find and replace functionality
  - Memory-efficient processing for files of any size
  - Preview changes before applying
  - Backup and undo support
  - Regular expressions with capture groups
- 📁 **File Filtering**: Flexible ignore patterns and file type filtering
- 📊 **Rich Output**: Detailed search results with statistics
- 📝 **Context Lines**: Show lines before and after matches for better understanding
  - `--context-before N` or `-B N`: Show N lines before each match
  - `--context-after N` or `-A N`: Show N lines after each match
  - `--context N` or `-C N`: Show N lines before and after each match
- 🛠️ **Developer Friendly**: Clear documentation with .NET comparison examples

## Installation

### From crates.io

```bash
# Install CLI tool
cargo install rustscout-cli

# Or add library to your project
cargo add rustscout
```

### From Source

```bash
git clone https://github.com/willibrandon/rustscout.git
cd rustscout
cargo install --path rustscout-cli
```

## Quick Start

Basic usage:

```bash
# Simple text search
rustscout-cli "pattern" /path/to/search

# Search with word boundaries
rustscout-cli --pattern "add" --word-boundary . # Find "add" but not "address"

# Search with regex and word boundaries
rustscout-cli --pattern "test_.*" --word-boundary --regex . # Find test functions

# Note: If your regex already has \b markers (e.g., "\btest\b"), RustScout preserves them.
#       Otherwise, --word-boundary automatically adds them around your pattern.

# Filter by file type
rustscout-cli --extensions rs,toml "TODO" .

# Show only statistics
rustscout-cli --stats-only "FIXME" .

# Ignore specific patterns
rustscout-cli --ignore "target/*,*.tmp" "pattern" .

# Control thread count
rustscout-cli --threads 8 "pattern" .
```

## Usage Examples

### Basic Search

```bash
# Search for a pattern in current directory
rustscout-cli "pattern" .

# Search in a specific directory
rustscout-cli "pattern" /path/to/search

# Case-sensitive search
rustscout-cli "CamelCase" --case-sensitive .

# Show context lines around matches
rustscout-cli -C 2 "pattern" .  # 2 lines before and after
rustscout-cli -B 3 "pattern" .  # 3 lines before
rustscout-cli -A 2 "pattern" .  # 2 lines after
```

### Advanced Pattern Matching

#### Basic Regex Examples

```bash
# Find function definitions
rustscout-cli "fn\s+\w+\s*\([^)]*\)" .

# Find standalone TODO comments
rustscout-cli --pattern "TODO" --word-boundary .

# Multiple patterns with word boundaries
rustscout-cli --pattern "add" --word-boundary --pattern "remove" --word-boundary .

# Mix of patterns with different settings
rustscout-cli --pattern "test" --word-boundary --pattern "FIXME:.*bug.*line \d+" --regex .
```

#### Hyphen and Underscore Handling

```bash
# Smart hyphen handling (--hyphen-mode flag)
rustscout-cli --pattern "test" --word-boundary .                    # Won't match in "test-case" (default: code/joining mode)
rustscout-cli --pattern "hello" --word-boundary --hyphen-mode=boundary .  # Will match in "hello-world" (boundary mode for text)
rustscout-cli --pattern "test" --word-boundary --hyphen-mode=joining .    # Won't match in "test-case" (explicit joining mode)

# Underscore handling (always joins in all modes)
rustscout-cli --pattern "test" --word-boundary .          # Won't match in "test_case" (underscores always join)
rustscout-cli --pattern "hello_world" --word-boundary .   # Matches full identifier only
rustscout-cli --pattern "test_café_안녕" --word-boundary . # Matches full mixed-script identifier
```

#### Unicode Hyphen Support

```bash
# Matches with any hyphen type:
rustscout-cli --pattern "hello" --word-boundary --hyphen-mode=boundary .  
  # - ASCII hyphen-minus (U+002D)
  # - Unicode hyphen (U+2010)
  # - Non-breaking hyphen (U+2011)
  # - Figure dash (U+2012)
  # - En dash (U+2013)
```

#### Regex with Word Boundaries

```bash
# Explicit \b in pattern vs. --word-boundary flag
rustscout-cli --pattern "\bhello-\w+\b" --regex --hyphen-mode=boundary .  # \b matches word boundaries in pattern
rustscout-cli --pattern "test-\d+" --regex --word-boundary .              # --word-boundary adds \b outside pattern

# Pattern with no word boundaries
rustscout-cli --pattern "address" .                        # Matches "address" within words (e.g., "preaddress")
```

#### Unicode Word Boundaries

```bash
# Unicode-aware word boundaries
rustscout-cli --pattern "café" --word-boundary .          # Matches "café" but not "café-bar"
rustscout-cli --pattern "привет" --word-boundary .        # Works with Cyrillic
rustscout-cli --pattern "안녕" --word-boundary .          # Works with Korean
rustscout-cli --pattern "你好" --word-boundary .          # Works with Chinese

# Mixed-script identifiers (underscore always joins different scripts)
rustscout-cli --pattern "hello_世界" --word-boundary .    # Smart script bridging with underscore
rustscout-cli --pattern "test_café_안녕" --word-boundary . # Complex mixed-script cases
rustscout-cli --pattern "my_∑_total" --word-boundary .    # Math symbols in identifiers
```

### Incremental Search

```bash
# Enable incremental search with default settings
rustscout-cli search "TODO" --incremental

# Specify cache location
rustscout-cli search "TODO" --incremental --cache-path .rustscout/cache.json

# Choose change detection strategy
rustscout-cli search "TODO" --incremental --cache-strategy git

# Enable cache compression
rustscout-cli search "TODO" --incremental --use-compression

# Set cache size limit
rustscout-cli search "TODO" --incremental --max-cache-size 100MB
```

### Search and Replace

```bash
# Simple text replacement
rustscout-cli replace "old_text" --replace "new_text" src/*.rs

# Preview changes before applying
rustscout-cli replace "TODO" --replace "DONE" --preview src/      # See changes in console
rustscout-cli replace "TODO" --replace "DONE" --preview --dry-run # Preview without modifying files

# Replace with regex and capture groups
rustscout-cli replace --regex "fn\s+(\w+)" --capture-groups "fn new_$1" src/

# Complete backup and undo workflow
rustscout-cli replace "old_api" --replace "new_api" --backup src/     # Creates backup and records undo info
rustscout-cli list-undo                                              # Shows available undo operations with IDs
rustscout-cli undo --dry-run 1627384952                             # Preview what would be restored
rustscout-cli undo 1627384952                                       # Restore from backup using undo ID

# Preserve file metadata
rustscout-cli replace "pattern" --replace "new" --preserve src/

# Custom backup directory
rustscout-cli replace "pattern" --replace "new" --backup --output-dir backups/ src/

# Examples of validation behavior (with descriptive errors)
rustscout-cli replace "" --replace "test"        # Error: Empty pattern not allowed - prevents accidental mass replacements
rustscout-cli replace "[invalid" --replace "test" --regex  # Error: Invalid regex pattern - missing closing bracket
rustscout-cli replace "(\w+)" --replace "$2" --regex      # Error: Invalid capture group $2 - only $1 exists
rustscout-cli replace "test" --replace "new" --overlapping # Error: Overlapping replacements detected at line 42

# File size processing strategies (configurable thresholds)
rustscout-cli replace "pattern" --replace "new" --small-file-threshold 1MB src/  # Memory mapping for files < 1MB
rustscout-cli replace "pattern" --replace "new" --large-file-threshold 100MB src/ # Streaming for files > 100MB

# Undo system features
rustscout-cli list-undo --format json   # View undo information in JSON format
rustscout-cli undo --dry-run <id>       # Preview what would be restored
rustscout-cli undo --all                # Revert all changes in chronological order
```

### Validation and Safety Features

> As of v1.1.0, RustScout includes enhanced validation and safety features to ensure reliable replacements. For the full story behind these improvements, check out our [blog post on the replace module journey](docs/blog/2025-01-replace-module-enhancements.md).

RustScout includes robust validation and safety features to ensure reliable replacements:

- **Pattern Validation**
  - Empty patterns are rejected to prevent accidental mass replacements
  - Regex patterns are validated before execution with clear error messages
  - Capture group references are checked against available groups
  - Overlapping replacements are detected and prevented with line numbers

- **File Processing Safety**
  - Adaptive processing based on file size:
    - Small files (< 32KB by default): Memory mapping for speed
    - Medium files: Buffered reading with reasonable memory usage
    - Large files (> 10MB by default): Streaming for memory efficiency
  - Configurable thresholds via CLI flags or config file
  - Clear error messages guide you to the right processing strategy

- **Backup System**
  - Automatic backup creation before modifications
  - Backups stored with timestamps for easy identification
  - Custom backup directory support
  - Metadata preservation option

- **Undo System**
  - JSON-formatted undo logs for transparency
  - Dry-run option to preview restorations
  - Bulk undo support for reverting multiple changes
  - Chronological ordering of operations

```
Replace Pipeline:
Input -> Validate -> Backup -> Replace -> Record Undo
                                             |
                                             v
                                      [Restore if needed]
```

### File Filtering

```bash
# Search only Rust files
rustscout-cli --extensions rs "pattern" .

# Search multiple file types
rustscout-cli --extensions "rs,toml,md" "pattern" .

# Ignore specific patterns
rustscout-cli --ignore "target/*,*.tmp" "pattern" .
```

## Configuration

RustScout can be configured via a YAML file (`.rustscout.yaml`). Configuration files are loaded from multiple locations in order of precedence:

1. Custom config file specified via `--config` flag
2. Local `.rustscout.yaml` in the current directory
3. Global `$HOME/.config/rustscout/config.yaml`

Example configuration with explanations:

```yaml
# Search Patterns
# - Support for multiple patterns in a single search
# - Each pattern can be simple text or regex
# - Simple patterns use fast string matching
# - Regex patterns use full regex engine
patterns:
  - "TODO"                    # Simple text pattern
  - "FIXME"                   # Another simple pattern
  - "BUG-\\d+"                # Regex pattern with number
  - text: "test"              # Pattern with explicit settings
    is_regex: false
    boundary_mode: WholeWords # Match whole words only
  - text: "address"           # Pattern with no boundaries
    boundary_mode: None        # Match within words too

# Legacy single pattern support (using regex alternation)
pattern: "TODO|FIXME"

# Root directory to search in (default: ".")
root_path: "."

# File Extensions
# - Optional list to include (case-insensitive)
# - If not specified, searches all non-binary files
file_extensions:
  - "rs"
  - "toml"

# Ignore Patterns
# - Uses .gitignore syntax
# - Supports glob patterns
# - Built-in ignores: .git/, target/
ignore_patterns:
  - "target/**"              # Ignore target directory
  - ".git/**"                # Ignore git directory
  - "**/*.min.js"            # Ignore minified JS
  - "invalid.rs"             # Ignore any file named invalid.rs

# Ignore Pattern Syntax
RustScout uses a simplified `.gitignore`-like syntax:
- If the pattern **does not contain a slash**, it matches **only** the final file name.
  - Example: `invalid.rs` will match **any** file named `invalid.rs` in any directory.
- If the pattern **contains a slash**, it is interpreted as a glob pattern applied to the **entire path**.
  - Example: `tests/*.rs` matches `.rs` files in the `tests/` folder only.
  - Example: `**/invalid.rs` matches `invalid.rs` anywhere in the directory tree.

### Examples
- `invalid.rs` => Ignores any file with the exact name `invalid.rs`.
- `**/test_*.rs` => Ignores `test_foo.rs`, `test_bar.rs` in **any** subdirectory.
- `docs/*.md` => Ignores `.md` files in `docs/`, but not deeper subdirs like `docs/nested/`.

# Performance Settings
stats_only: false            # Show only statistics
thread_count: 4              # Number of threads (default: CPU cores)

# Logging
log_level: "info"            # trace, debug, info, warn, error

# Context Lines
context_before: 2            # Lines before matches
context_after: 2             # Lines after matches

# Incremental Search
incremental: false           # Enable incremental search
cache_path: ".rustscout/cache.json"
cache_strategy: "auto"       # "auto", "git", or "signature"
max_cache_size: "100MB"      # Optional size limit
use_compression: false       # Enable cache compression

# File Size Processing Strategies
processing:
  small_file_threshold: 32KB    # Default: 32KB
  large_file_threshold: 10MB    # Default: 10MB

# Undo System
undo:
  - "old_api"
  - "new_api"
```

### Command-Line Options

```bash
USAGE:
    rustscout-cli [SUBCOMMAND]
    rustscout-cli search [OPTIONS] [PATTERN] [ROOT_PATH]
    rustscout-cli replace [OPTIONS] <PATTERN> <FILES>...
    rustscout-cli list-undo
    rustscout-cli undo <ID>

SUBCOMMANDS:
    search       Search for patterns in files
    replace      Search and replace patterns in files
    list-undo    List available undo operations
    undo         Undo a previous replacement operation

SEARCH OPTIONS:
    [PATTERN]                      Pattern to search for (supports regex)
    [ROOT_PATH]                    Root directory to search in [default: .]
    -p, --pattern <PATTERN>        Pattern to search for (can be specified multiple times)
    -e, --extensions <EXTENSIONS>  Comma-separated list of file extensions to search (e.g. "rs,toml")
    -i, --ignore <PATTERNS>        Glob patterns to ignore
    -c, --case-sensitive           Enable case-sensitive search
    -s, --stats-only               Show only statistics
    -t, --threads <COUNT>          Number of threads to use
    -B, --context-before <LINES>   Lines of context before matches
    -A, --context-after <LINES>    Lines of context after matches
    -C, --context <LINES>          Lines of context around matches
    --incremental                  Enable incremental search
    --cache-path <PATH>            Path to store search cache [default: .rustscout/cache.json]
    --cache-strategy <STRATEGY>    Change detection strategy: auto, git, or signature [default: auto]
    --max-cache-size <SIZE>        Maximum cache size (e.g. "100MB")
    --use-compression              Enable cache compression

REPLACE OPTIONS:
    <PATTERN>                        Pattern to search for
    <FILES>...                       Files or directories to process
    -r, --replace <TEXT>             The replacement text
    -R, --regex                      Use regex pattern matching
    -g, --capture-groups <GROUPS>    Use capture groups in the replacement (e.g. "$1, $2")
    -n, --dry-run                    Show what would be changed without modifying files
    -b, --backup                     Create backups of modified files
    -o, --output-dir <DIR>           Directory for backups and temporary files
    -p, --preview                    Show detailed preview of changes
    --preserve                       Preserve file permissions and timestamps
    -t, --threads <COUNT>            Number of threads to use
    -l, --log-level <LEVEL>          Log level (trace, debug, info, warn, error) [default: warn]

GLOBAL OPTIONS:
    -h, --help                       Print help information
    -V, --version                    Print version information
```

## Library Usage

RustScout can also be used as a library in your Rust projects:

```toml
[dependencies]
rustscout = "0.1.0"
```

### Search Example

```rust
use rustscout::{SearchConfig, search, WordBoundaryMode, HyphenHandling};
use std::num::NonZeroUsize;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // Basic search with word boundaries (code mode)
    let config = SearchConfig::new_with_pattern(
        "test".to_string(),
        false,
        WordBoundaryMode::WholeWords,
    );

    // Advanced search with custom hyphen handling
    let config = SearchConfig {
        pattern_definitions: vec![
            PatternDefinition {
                text: "hello".to_string(),
                is_regex: false,
                boundary_mode: WordBoundaryMode::WholeWords,
                hyphen_handling: HyphenHandling::Boundary, // for natural text
            }
        ],
        // ... other settings ...
    };

    let result = search(&config)?;
    println!("Found {} matches", result.total_matches);
    Ok(())
}
```

### Replace Example

```rust
use rustscout::{FileReplacementPlan, ReplacementConfig, ReplacementSet, ReplacementTask};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // Configure the replacement operation
    let config = ReplacementConfig {
        pattern: "old_api".to_string(),
        replacement: "new_api".to_string(),
        is_regex: false,
        backup_enabled: true,
        dry_run: false,
        backup_dir: Some(PathBuf::from("backups")),
        preserve_metadata: true,
        capture_groups: None,
        undo_dir: PathBuf::from(".rustscout/undo"),
    };

    // Create a replacement set
    let mut replacement_set = ReplacementSet::new(config.clone());

    // Add files to process
    let mut plan = FileReplacementPlan::new("src/lib.rs".into())?;
    plan.add_replacement(ReplacementTask::new(
        "src/lib.rs".into(),
        (100, 107), // start and end positions
        "new_api".to_string(),
        config.clone(),
    ));
    replacement_set.add_plan(plan);

    // Preview changes
    for preview in replacement_set.preview()? {
        println!("Changes in {}:", preview.file_path.display());
        for (i, line) in preview.original_lines.iter().enumerate() {
            println!("- {}", line);
            println!("+ {}", preview.new_lines[i]);
        }
    }

    // Apply changes
    let undo_info = replacement_set.apply_with_progress()?;
    println!("Applied {} changes", undo_info.len());

    Ok(())
}
```

## Performance & Benchmarks

Performance comparison with other popular search tools (searching a large Rust codebase):

| Tool      | Time (ms) | Memory (MB) |
|-----------|-----------|-------------|
| RustScout | 120       | 15          |
| ripgrep   | 150       | 18          |
| grep      | 450       | 12          |

*Note: These are example benchmarks. Actual performance may vary based on the specific use case and system configuration.*

### Adaptive Processing Strategies

RustScout employs different processing strategies based on file size:

1. **Small Files (<32KB)**:
   - Direct string operations
   - ~330 µs for simple patterns
   - ~485 µs for regex patterns
   - Optimal for quick access to small files

2. **Medium Files (32KB - 10MB)**:
   - Buffered reading
   - ~1.4 ms for 10 files
   - ~696 µs for 50 files (parallel processing)
   - Good balance of memory usage and performance

3. **Large Files (>10MB)**:
   - Memory mapping for efficient access
   - ~2.1 ms for 20MB file with simple pattern
   - ~3.2 ms for 20MB file with regex pattern
   - Parallel pattern matching within files

### Pattern Optimization

1. **Pattern Caching**:
   - Global thread-safe pattern cache
   - Simple patterns: ~331 µs (consistent performance)
   - Regex patterns: ~483 µs (~0.7% improvement)
   - Zero-cost abstraction when no contention

2. **Smart Pattern Detection**:
   - Automatic detection of pattern complexity
   - Simple string matching for basic patterns
   - Full regex support for complex patterns
   - Threshold-based optimization

### Memory Usage Tracking

RustScout now includes comprehensive memory metrics:
- Total allocated memory and peak usage
- Memory mapped regions for large files
- Pattern cache size and hit/miss rates
- File processing statistics by size category

### Performance Tips

1. **Use Simple Patterns** when possible:

    ```bash
    # Faster - uses optimized literal search
    rustscout-cli "TODO" .

    # Slower - requires regex engine
    rustscout-cli "TODO.*FIXME" .
    ```

2. **Control Thread Count** based on your system:

    ```bash
    # Use all available cores (default)
    rustscout-cli "pattern" .

    # Limit to 4 threads for lower CPU usage
    rustscout-cli --threads 4 "pattern" .
    ```

3. **Filter File Types** to reduce search space:

    ```bash
    # Search only Rust and TOML files
    rustscout-cli --extensions rs,toml "pattern" .
    ```

4. **Monitor Memory Usage**:

    ```bash
    # Show memory usage statistics
    rustscout-cli "pattern" --stats .
    ```

## Troubleshooting

### Common Issues and Solutions

1. **Pattern Not Found**
   - **Issue**: Search returns no results
   - **Solutions**:
     - Check if pattern is case-sensitive
     - Verify file extensions are correctly specified
     - Check ignore patterns aren't too broad

2. **Performance Issues**
   - **Issue**: Search is slower than expected
   - **Solutions**:
     - Use simple patterns instead of complex regex
     - Adjust thread count with `--threads`
     - Filter specific file types with `--extensions`
     - Check if searching binary files (use `--stats-only` to verify)

3. **Permission Errors**
   - **Issue**: "Permission denied" errors
   - **Solutions**:
     - Run with appropriate permissions
     - Check file and directory access rights
     - Use ignore patterns to skip problematic directories

4. **Invalid Regex Pattern**
   - **Issue**: "Invalid regex pattern" error
   - **Solutions**:
     - Escape special characters: `\., \*, \+`
     - Use raw strings for Windows paths: `\\path\\to\\file`
     - Verify regex syntax at [regex101.com](https://regex101.com)

5. **Memory Usage**
   - **Issue**: High memory consumption
   - **Solutions**:
     - Use `--stats-only` for large codebases
     - Filter specific file types
     - Adjust thread count to limit concurrency

### Error Messages

| Error Message               | Cause                       | Solution                                                             |
|-----------------------------|-----------------------------|----------------------------------------------------------------------|
| Error: Invalid regex pattern | Malformed regex expression  | Check regex syntax and escape special characters                    |
| Error: Permission denied      | Insufficient file permissions | Run with appropriate permissions or ignore problematic paths         |
| Error: File too large         | File exceeds size limit      | Use `--stats-only` or filter by file type                           |
| Error: Invalid thread count   | Invalid `--threads` value    | Use a positive number within system limits                           |
| Error: Invalid file extension | Malformed extension filter   | Use comma-separated list without spaces                              |

## Development Process

RustScout represents an interesting experiment in AI-assisted software development. The entire codebase was primarily developed through collaboration with AI language models, with human oversight focusing on:

- Project direction and requirements
- Design decisions and architecture
- Testing and validation
- User experience

This approach demonstrates how AI can be leveraged to:

1. Bootstrap complex software projects
2. Implement best practices and patterns
3. Handle sophisticated technical implementations
4. Maintain consistency across a growing codebase

The project serves as a case study in AI-driven development, showing that with proper orchestration, AI can produce production-quality code while adhering to language idioms and best practices. Notably, this was achieved without the human overseer having prior Rust experience, illustrating how AI can bridge the gap between concept and implementation.

### Development Principles

- AI handles implementation details
- Humans focus on high-level direction
- Continuous testing and validation
- Emphasis on maintainable, documented code
- Regular review and refinement cycles

This transparent approach to development aims to:

1. Demonstrate the capabilities of AI-assisted development
2. Provide insights into new software development methodologies
3. Encourage discussion about AI's role in software engineering
4. Show how AI can complement human expertise

We welcome contributions and discussions about both the codebase and the development methodology.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.