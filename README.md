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
- 🔍 **Smart Search**: Support for multiple patterns with mix of simple text and regex
- 📁 **File Filtering**: Flexible ignore patterns and file type filtering
- 📊 **Rich Output**: Detailed search results with statistics
- 🛠️ **Developer Friendly**: Clear documentation with .NET comparison examples
- 📝 **Context Lines**: Show lines before and after matches for better understanding
  - `--context-before N` or `-B N`: Show N lines before each match
  - `--context-after N` or `-A N`: Show N lines after each match
  - `--context N` or `-C N`: Show N lines before and after each match

## Quick Start

Install RustScout using cargo:

```bash
cargo install rustscout-cli
```

Basic usage:

```bash
# Simple text search
rustscout-cli "pattern" /path/to/search

# Search with regex
rustscout-cli "fn\s+\w+\s*\([^)]*\)" . # Find function definitions

# Filter by file type
rustscout-cli --extensions rs,toml "TODO" .

# Show only statistics
rustscout-cli --stats-only "FIXME" .

# Ignore specific patterns
rustscout-cli --ignore "target/*,*.tmp" "pattern" .

# Control thread count
rustscout-cli --threads 8 "pattern" .
```

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
```bash
# Find function definitions
rustscout-cli "fn\s+\w+\s*\([^)]*\)" .

# Find TODO comments
rustscout-cli "TODO:.*" .

# Multiple simple patterns
rustscout-cli --pattern "TODO" --pattern "FIXME" .

# Mix of simple and regex patterns
rustscout-cli --pattern "TODO" --pattern "FIXME:.*bug.*line \d+" .
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

### Output Control
```bash
# Show only statistics
rustscout-cli --stats-only "pattern" .

# Show line numbers
rustscout-cli --line-numbers "pattern" .
```

### Performance Tuning
```bash
# Set thread count
rustscout-cli --threads 8 "pattern" .

# Process large files
rustscout-cli --chunk-size 1024 "pattern" .
```

## Library Usage

RustScout can also be used as a library in your Rust projects:

```toml
[dependencies]
rustscout = "0.1.0"
```

```rust
use rustscout::{Config, search};
use std::num::NonZeroUsize;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let config = Config {
        pattern: "TODO".to_string(),
        root_path: PathBuf::from("."),
        thread_count: NonZeroUsize::new(8).unwrap(),
        ignore_patterns: vec!["target/*".to_string()],
        file_extensions: Some(vec!["rs".to_string()]),
        stats_only: false,
    };

    let results = search(&config)?;
    println!("Found {} matches", results.total_matches);
    Ok(())
}
```

## Configuration

`rustscout` supports flexible configuration through both YAML configuration files and command-line arguments. Command-line arguments take precedence over configuration file values.

### Configuration Locations

Configuration files are loaded from multiple locations in order of precedence:
1. Custom config file specified via `--config` flag
2. Local `.rustscout.yaml` in the current directory
3. Global `$HOME/.config/rustscout/config.yaml`

### Configuration Format

Example `.rustscout.yaml`:
```yaml
# Search patterns (supports both simple text and regex)
patterns:
  - "TODO"
  - "FIXME"
  - "BUG-\\d+"

# Legacy single pattern support
pattern: "TODO|FIXME"

# Root directory to search in
root_path: "."

# File extensions to include
file_extensions:
  - "rs"
  - "toml"

# Patterns to ignore (glob syntax)
ignore_patterns:
  - "target/**"
  - ".git/**"
  - "**/*.min.js"

# Show only statistics
stats_only: false

# Thread count (default: CPU cores)
thread_count: 4

# Log level (trace, debug, info, warn, error)
log_level: "info"

# Context lines before matches
context_before: 2

# Context lines after matches
context_after: 2
```

### Command-Line Options

```bash
USAGE:
    rustscout-cli [OPTIONS] [PATTERN] [ROOT_PATH]

ARGS:
    [PATTERN]      Pattern to search for (supports regex)
    [ROOT_PATH]    Root directory to search in [default: .]

OPTIONS:
    -p, --pattern <PATTERN>         Pattern to search for (can be specified multiple times)
    -e, --extensions <EXTENSIONS>    Comma-separated list of file extensions to search (e.g. "rs,toml")
    -i, --ignore <PATTERNS>         Additional patterns to ignore (supports .gitignore syntax)
    --stats-only                    Show only statistics, not individual matches
    -t, --threads <COUNT>           Number of threads to use for searching
    -l, --log-level <LEVEL>         Log level (trace, debug, info, warn, error) [default: warn]
    -c, --config <FILE>             Path to config file [default: .rustscout.yaml]
    -B, --context-before <LINES>    Number of lines to show before each match [default: 0]
    -A, --context-after <LINES>     Number of lines to show after each match [default: 0]
    -C, --context <LINES>           Number of lines to show before and after each match
    -h, --help                      Print help information
    -V, --version                   Print version information
```

### Configuration Details

#### Search Patterns
- Supports multiple patterns in a single search
- Each pattern can be simple text or regex
- Simple patterns use fast string matching
- Regex patterns use the full regex engine
- Examples:
  - Simple: `["TODO", "FIXME"]`
  - Mixed: `["TODO", "FIXME:.*\\b\\d{4}\\b"]`
  - Legacy: `"TODO|FIXME"` (using regex alternation)

#### File Extensions
- Optional list of extensions to include
- Case-insensitive matching
- If not specified, searches all non-binary files

#### Ignore Patterns
- Uses `.gitignore` syntax
- Supports glob patterns
- Built-in ignores: `.git/`, `target/`
- Examples:
  - `**/node_modules/**`
  - `*.bak`
  - `build/*.o`

#### Thread Control
- Default: Number of CPU cores
- Can be reduced for lower system impact
- Can be increased for faster searching on I/O-bound systems

#### Log Levels
- `trace`: Most verbose, shows all operations
- `debug`: Shows detailed progress
- `info`: Shows important progress
- `warn`: Shows only warnings (default)
- `error`: Shows only errors

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Benchmarks

Performance comparison with other popular search tools (searching a large Rust codebase):

| Tool      | Time (ms) | Memory (MB) |
|-----------|-----------|-------------|
| RustScout | 120       | 15          |
| ripgrep   | 150       | 18          |
| grep      | 450       | 12          |

*Note: These are example benchmarks. Actual performance may vary based on the specific use case and system configuration.*

## Performance

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

## Troubleshooting Guide

### Common Issues and Solutions

1. **Pattern Not Found**
   - Issue: Search returns no results
   - Solutions:
     - Check if pattern is case-sensitive
     - Verify file extensions are correctly specified
     - Check ignore patterns aren't too broad

2. **Performance Issues**
   - Issue: Search is slower than expected
   - Solutions:
     - Use simple patterns instead of complex regex
     - Adjust thread count with `--threads`
     - Filter specific file types with `--extensions`
     - Check if searching binary files (use `--stats-only` to verify)

3. **Permission Errors**
   - Issue: "Permission denied" errors
   - Solutions:
     - Run with appropriate permissions
     - Check file and directory access rights
     - Use ignore patterns to skip problematic directories

4. **Invalid Regex Pattern**
   - Issue: "Invalid regex pattern" error
   - Solutions:
     - Escape special characters: `\.`, `\*`, `\+`
     - Use raw strings for Windows paths: `\\path\\to\\file`
     - Verify regex syntax at [regex101.com](https://regex101.com)

5. **Memory Usage**
   - Issue: High memory consumption
   - Solutions:
     - Use `--stats-only` for large codebases
     - Filter specific file types
     - Adjust thread count to limit concurrency

### Error Messages

| Error Message | Cause | Solution |
|--------------|-------|----------|
| `Error: Invalid regex pattern` | Malformed regex expression | Check regex syntax and escape special characters |
| `Error: Permission denied` | Insufficient file permissions | Run with appropriate permissions or ignore problematic paths |
| `Error: File too large` | File exceeds size limit | Use `--stats-only` or filter by file type |
| `Error: Invalid thread count` | Invalid `--threads` value | Use a positive number within system limits |
| `Error: Invalid file extension` | Malformed extension filter | Use comma-separated list without spaces | 