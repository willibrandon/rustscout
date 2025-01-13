# RTrace 🚀

[![Crates.io](https://img.shields.io/crates/v/rtrace_cli.svg)](https://crates.io/crates/rtrace_cli)
[![Build Status](https://github.com/yourusername/rtrace/workflows/CI/badge.svg)](https://github.com/yourusername/rtrace/actions)
[![License](https://img.shields.io/crates/l/rtrace_cli.svg)](LICENSE)

> Lightning-fast code search with smart pattern matching and parallel processing

RTrace is a modern code search tool written in Rust that delivers blazing-fast performance through intelligent pattern detection and adaptive parallel processing. It automatically optimizes search strategies based on file sizes and pattern complexity.

## ⚡ Performance

Recent benchmarks show significant performance improvements:

### Pattern Matching Speed
- **46% faster** for comment patterns
- **39% faster** for simple patterns
- **15% faster** for complex regex patterns

### File Processing
- **27% faster** for small codebases (10 files)
- **16% faster** for medium projects (50 files)
- **44% faster** parallel processing across all thread counts

## 🚀 Quick Start

### Installation

```bash
# Install from crates.io
cargo install rtrace_cli

# Or download pre-built binaries from releases
```

### Basic Usage

```bash
# Search for a pattern in current directory
rtrace "TODO"

# Search in specific directory with file extension filter
rtrace "fn.*\(" --path src --ext rs

# Search with ignore patterns
rtrace "TODO" --ignore "target/**" --ignore ".git/**"
```

## ✨ Features

- **Smart Pattern Detection**: Automatically uses optimized search strategies for different pattern types
- **Parallel Processing**: Efficiently utilizes all CPU cores with consistent performance scaling
- **Adaptive Search**: Optimizes for both small and large files
- **Rich Output**: Colored matches with line numbers and context
- **Flexible Filtering**: File extension and ignore pattern support
- **Memory Efficient**: Streaming processing for large files

## 🔍 Advanced Usage

### Pattern Types

RTrace automatically detects pattern complexity and uses the most efficient search strategy:

```bash
# Simple literal search (fastest)
rtrace "findThis"

# Word boundary search
rtrace "\bword\b"

# Complex regex pattern
rtrace "fn\s+\w+\s*\([^)]*\)"
```

### File Filtering

```bash
# Multiple file extensions
rtrace "pattern" --ext rs,go,js

# Complex ignore patterns
rtrace "pattern" --ignore "**/*.min.js" --ignore "node_modules/**"
```

### Performance Tuning

```bash
# Specify thread count for parallel processing
rtrace "pattern" --threads 8

# Statistics only mode for large searches
rtrace "pattern" --stats-only
```

## 🔧 Configuration

RTrace can be configured through command line flags or a config file:

```toml
# .rtrace.toml
extensions = ["rs", "go", "js"]
ignore_patterns = ["target/**", ".git/**"]
thread_count = 8
```

## 🤝 Contributing

We welcome contributions! See our [Contributing Guide](CONTRIBUTING.md) for details.

### Good First Issues

- [ ] Add support for custom output formats
- [ ] Implement file size-based buffer tuning
- [ ] Add more test cases

## 📈 Benchmarks

Detailed performance comparisons:

```
Pattern Matching (μs)
┌─────────────┬────────┬─────────┬──────────┐
│ Pattern     │ Before │  After  │ Speedup  │
├─────────────┼────────┼─────────┼──────────┤
│ Simple      │   576  │   347   │   39%    │
│ Complex     │   698  │   603   │   14%    │
│ Comments    │   557  │   302   │   46%    │
└─────────────┴────────┴─────────┴──────────┘

File Processing (μs)
┌─────────────┬────────┬─────────┬──────────┐
│ Files       │ Before │  After  │ Speedup  │
├─────────────┼────────┼─────────┼──────────┤
│ 10 files    │   423  │   308   │   27%    │
│ 50 files    │   807  │   675   │   16%    │
│ 100 files   │  1160  │  1103   │    5%    │
└─────────────┴────────┴─────────┴──────────┘
```

## ⭐ Support

If you find RTrace useful, please consider giving it a star on GitHub! Your support helps us improve the tool and add more features.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 