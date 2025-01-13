# RTrace

[![CI](https://github.com/willibrandon/rtrace/actions/workflows/ci.yml/badge.svg)](https://github.com/willibrandon/rtrace/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/willibrandon/rtrace/branch/main/graph/badge.svg)](https://codecov.io/gh/willibrandon/rtrace)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance, cross-platform CLI tool for searching and analyzing large codebases or text files. RTrace leverages Rust's concurrency features to provide fast, parallel search capabilities.

## Features

- 🚀 **Concurrent File Scanning:** Automatically distributes scanning across multiple threads
- 🎯 **File Type Filters:** Search specific file types (e.g., `*.rs`, `*.cs`, `*.fs`)
- 🔍 **Regex-Based Search:** Fast pattern matching with regular expressions
- 📊 **Result Summaries:** Detailed matches or high-level statistics
- ⚙️ **Configurable Options:** Control threads, ignore patterns, and output format
- 🖥️ **Cross-Platform:** Works on Windows, Linux, and macOS

## Installation

### Using Cargo

```bash
cargo install rtrace_cli
```

### From Source

```bash
git clone https://github.com/willibrandon/rtrace.git
cd rtrace
cargo build --release
```

The binary will be available at `target/release/rtrace_cli`.

## Usage

Basic search:
```bash
rtrace_cli --pattern "TODO|FIXME" --path ./src
```

Search with specific file types:
```bash
rtrace_cli --pattern "fn.*main" --path ./src --extensions rs,toml
```

Get only statistics:
```bash
rtrace_cli --pattern "unsafe" --path ./src --stats-only
```

Control thread count:
```bash
rtrace_cli --pattern "error" --path ./logs --threads 8
```

### Command Line Options

- `--pattern <regex>`: Search pattern (regular expression)
- `--path <dir>`: Directory to search in
- `--threads <n>`: Number of threads to use (default: number of CPU cores)
- `--ignore <pattern>`: Ignore files/directories matching pattern
- `--stats-only`: Show only summary statistics
- `--extensions <list>`: Comma-separated list of file extensions to search

## For .NET Developers

RTrace demonstrates several Rust concepts that parallel .NET patterns:

- **Concurrency:** Uses Rayon's parallel iterators (similar to .NET's Parallel.ForEach)
- **Error Handling:** Result types (analogous to Try/Catch but more explicit)
- **Modularity:** Workspace with multiple crates (similar to .NET solution with multiple projects)

## Development

### Project Structure

- `rtrace_cli/`: Binary crate for the command-line interface
- `rtrace_core/`: Library crate with the core search functionality

### Building and Testing

Run tests:
```bash
cargo test
```

Run benchmarks:
```bash
cargo bench
```

Check formatting:
```bash
cargo fmt --all -- --check
```

Run linter:
```bash
cargo clippy
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 