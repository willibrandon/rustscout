# RustScout

[![Crates.io](https://img.shields.io/crates/v/rustscout.svg)](https://crates.io/crates/rustscout)
[![Crates.io](https://img.shields.io/crates/v/rustscout-cli.svg)](https://crates.io/crates/rustscout-cli)
[![Documentation](https://docs.rs/rustscout/badge.svg)](https://docs.rs/rustscout)
[![License:MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://github.com/willibrandon/rustscout/workflows/CI/badge.svg)](https://github.com/willibrandon/rustscout/actions)

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
rustscout-cli "pattern" /path/to/search
```

For more options:

```bash
rustscout-cli --help
```

## Installation

### From crates.io

```bash
cargo install rustscout-cli
```

### From Source

```bash
git clone https://github.com/willibrandon/rustscout.git
cd rustscout
cargo install --path rtrace_cli
```

## Usage

### Basic Search
```bash
rustscout-cli "search pattern" .
```

### With File Type Filter
```bash
rustscout-cli "pattern" . --type rs,toml
```

### Ignore Patterns
```bash
rustscout-cli "pattern" . --ignore "target/*"
```

### Statistics Only
```bash
rustscout-cli "pattern" . --stats-only
```

## Library Usage

RustScout can also be used as a library in your Rust projects:

```toml
[dependencies]
rustscout = "0.1.0"
```

```rust
use rustscout::search;

fn main() {
    let results = search("pattern", ".", None).unwrap();
    println!("Found {} matches", results.total_matches);
}
```

## Configuration

RustScout can be configured via command line arguments or configuration files. See the [documentation](https://docs.rs/rustscout) for more details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

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