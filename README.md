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
- 🔍 **Smart Search**: Regex support with intelligent pattern matching
- 📁 **File Filtering**: Flexible ignore patterns and file type filtering
- 📊 **Rich Output**: Detailed search results with statistics
- 🛠️ **Developer Friendly**: Clear documentation with .NET comparison examples

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
```

### Advanced Pattern Matching
```bash
# Find function definitions
rustscout-cli "fn\s+\w+\s*\([^)]*\)" .

# Find TODO comments
rustscout-cli "TODO:.*" .

# Find multiple patterns
rustscout-cli "(FIXME|TODO|XXX):" .
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
# Search pattern (supports regex)
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
```

### Command-Line Options

```bash
USAGE:
    rustscout-cli [OPTIONS] <PATTERN> [ROOT_PATH]

ARGS:
    <PATTERN>      Pattern to search for (supports regex)
    [ROOT_PATH]    Root directory to search in [default: .]

OPTIONS:
    -e, --extensions <EXTENSIONS>    Comma-separated list of file extensions to search (e.g. "rs,toml")
    -i, --ignore <PATTERNS>         Additional patterns to ignore (supports .gitignore syntax)
    --stats-only                    Show only statistics, not individual matches
    -t, --threads <COUNT>           Number of threads to use for searching
    -l, --log-level <LEVEL>         Log level (trace, debug, info, warn, error) [default: warn]
    -c, --config <FILE>             Path to config file [default: .rustscout.yaml]
    -h, --help                      Print help information
    -V, --version                   Print version information
```

### Configuration Details

#### Search Pattern
- Supports both simple text and regex patterns
- For regex patterns, uses the Rust regex syntax
- Examples:
  - Simple: `TODO`
  - Regex: `FIXME:.*\b\d{4}\b`

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

### Benchmark Results

rustscout has been benchmarked against various scenarios using Criterion:

1. **Simple Pattern Search** (searching for "TODO" in 10 files with 100 lines each):
   - Average time: ~2ms
   - Consistent performance across multiple runs
   - Linear scaling with file size

2. **Regex Pattern Search** (searching for complex patterns like `FIXME:.*bug.*line \d+`):
   - Average time: ~5ms
   - Optimized for both simple and complex patterns
   - Automatic pattern optimization for literal strings

3. **File Count Scaling** (searching across different numbers of files):
   - 5 files: ~1ms
   - 10 files: ~2ms
   - 25 files: ~4ms
   - 50 files: ~8ms
   - Near-linear scaling with file count

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