# RustScout Developer Guide

This guide explains the internals of RustScout and how to extend its functionality. It's particularly focused on helping .NET developers understand Rust patterns through familiar parallels.

## Architecture Overview

RustScout is organized into two main crates:

1. `rustscout` (Library Crate)
   - Core search functionality
   - Concurrency implementation
   - File filtering
   - Result aggregation

2. `rustscout-cli` (Binary Crate)
   - Command-line interface
   - Argument parsing
   - Output formatting

### Key Modules

- `config.rs`: Configuration management
- `errors.rs`: Custom error types
- `filters.rs`: File filtering logic
- `results.rs`: Search result types
- `search.rs`: Core search implementation

## Concurrency Implementation

### .NET vs Rust Approach

In .NET, you might use the Task Parallel Library (TPL):
```csharp
var results = files.AsParallel()
    .Select(file => SearchFile(file))
    .Where(result => result.Matches.Any())
    .ToList();
```

RustScout uses Rayon for similar functionality:
```rust
let results: Vec<_> = files.par_iter()
    .map(|file| search_file(file))
    .filter_map(|r| r.ok())
    .filter(|r| !r.matches.is_empty())
    .collect();
```

### Performance Optimizations

1. **File Size Stratification**
   - Small files (<32KB) use simple string search
   - Large files use chunked processing
   - Similar to .NET's partitioning strategies

2. **Pattern-Based Strategy**
   - Simple patterns use fast literal search
   - Complex patterns use compiled regex
   - Analogous to .NET's Regex.IsMatch optimization

3. **Work Stealing**
   - Uses Rayon's work-stealing thread pool
   - Automatically balances workload
   - Similar to TPL's work-stealing scheduler

## Error Handling

### Rust vs .NET Approach

.NET uses exceptions:
```csharp
try {
    var result = SearchFiles(pattern);
} catch (IOException ex) {
    // Handle error
}
```

RustScout uses Result types:
```rust
match search(&config) {
    Ok(result) => // Process result,
    Err(SearchError::FileNotFound(path)) => // Handle missing file,
    Err(SearchError::PermissionDenied(path)) => // Handle permission error,
    Err(e) => // Handle other errors
}
```

## Extending RustScout

### Adding New File Filters

1. Update `filters.rs`:
```rust
pub fn my_custom_filter(path: &Path, config: &Config) -> bool {
    // Implement filter logic
}
```

2. Integrate with existing filters:
```rust
pub fn should_include_file(path: &Path, config: &Config) -> bool {
    !is_likely_binary(path)
        && has_valid_extension(path, &config.file_extensions)
        && !should_ignore(path, &config.ignore_patterns)
        && my_custom_filter(path, config)
}
```

### Custom Output Formats

1. Implement a new output formatter:
```rust
pub struct JsonFormatter;

impl ResultFormatter for JsonFormatter {
    fn format(&self, result: &SearchResult) -> String {
        serde_json::to_string(result).unwrap()
    }
}
```

2. Use in CLI:
```rust
let formatter = match args.format {
    Format::Json => Box::new(JsonFormatter),
    Format::Text => Box::new(TextFormatter),
};
println!("{}", formatter.format(&result));
```

## Memory Management

### .NET vs Rust Patterns

.NET relies on garbage collection:
```csharp
using var reader = new StreamReader(path);
var buffer = new StringBuilder();
```

RustScout uses explicit buffer management:
```rust
let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
let mut line_buffer = String::with_capacity(256);
```

## Testing

### Unit Tests

Each module has its own tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_simple_pattern() {
        // Test implementation
    }
}
```

### Integration Tests

Test complete workflows:
```rust
#[test]
fn test_search_with_filters() {
    let config = Config {
        pattern: "TODO".to_string(),
        root_path: PathBuf::from("."),
        // ...
    };
    let result = search(&config).unwrap();
    assert!(result.total_matches > 0);
}
```

### Benchmarks

Performance tests using Criterion:
```rust
fn bench_simple_pattern(c: &mut Criterion) {
    // Benchmark implementation
}

criterion_group!(benches, bench_simple_pattern);
criterion_main!(benches);
```

## Best Practices

1. **Error Handling**
   - Use custom error types
   - Provide context with error messages
   - Handle all error cases explicitly

2. **Performance**
   - Profile before optimizing
   - Use appropriate buffer sizes
   - Leverage parallel processing

3. **Documentation**
   - Include .NET comparisons
   - Explain Rust-specific patterns
   - Provide clear examples

## Common Tasks

### Adding a New Command-Line Option

1. Update `Args` struct in `main.rs`:
```rust
#[derive(Parser)]
struct Args {
    #[arg(long)]
    my_option: String,
}
```

2. Update `Config` struct:
```rust
pub struct Config {
    pub my_option: String,
}
```

3. Pass through to search function:
```rust
let config = Config {
    my_option: args.my_option,
    // ...
};
```

### Adding a New Search Feature

1. Update `Config` struct
2. Implement search logic in `search.rs`
3. Add tests
4. Update documentation

## Troubleshooting

Common issues and solutions:

1. **Performance Issues**
   - Check file size stratification
   - Verify thread count configuration
   - Profile with large files

2. **Memory Usage**
   - Monitor buffer sizes
   - Check for unnecessary clones
   - Use appropriate string capacity

3. **Concurrency Problems**
   - Verify thread safety
   - Check for deadlocks
   - Monitor thread pool usage

## Resources

- [Rust Documentation](https://doc.rust-lang.org/)
- [Rayon Guide](https://docs.rs/rayon/)
- [Criterion User Guide](https://bheisler.github.io/criterion.rs/book/) 