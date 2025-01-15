# Word Boundary Search in RustScout: Precise Pattern Matching

Today, we're introducing word boundary search in RustScout, a powerful feature that enables more precise pattern matching. This enhancement allows users to find exact word matches while avoiding partial matches, making code search more accurate and efficient.

## The Challenge: Precise Pattern Matching

Code search often requires finding specific identifiers or keywords. However, simple pattern matching can lead to unwanted partial matches:
- Searching for "test" matches "testing", "attestation", "contest"
- Looking for "TODO" matches "TODOS", "TODOLIST", "AUTODO"
- Finding "add" matches "address", "padding", "ladder"

These partial matches create noise in search results, making it harder to find exactly what you're looking for.

## The Solution: Smart Word Boundary Detection

We've implemented a flexible word boundary system that works with both simple and regex patterns:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBoundaryMode {
    /// No boundary checking (existing behavior)
    None,
    /// Uses word boundary checks (\b or equivalent)
    WholeWords,
}
```

The implementation adapts to the pattern type:

```rust
impl PatternMatcher {
    fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        match self.strategy {
            MatchStrategy::Simple { pattern, boundary_mode } => {
                text.match_indices(pattern)
                    .filter(|(start, matched)| {
                        match boundary_mode {
                            WordBoundaryMode::None => true,
                            WordBoundaryMode::WholeWords => 
                                Self::is_word_boundary(text, *start, start + matched.len()),
                        }
                    })
                    .collect()
            }
            MatchStrategy::Regex { regex, .. } => {
                // Word boundaries handled in regex pattern itself
                regex.find_iter(text)
                    .map(|m| (m.start(), m.end()))
                    .collect()
            }
        }
    }
}
```

## Usage Examples

The feature is accessible through the CLI with the `-w` flag:

```bash
# Find standalone "TODO" comments
rustscout search -p TODO -w true

# Search for the "add" function without matching "address"
rustscout search -p add -w true

# Combine with regex for more power
rustscout search -p "test_.*" -w true -r
```

## Implementation Details

The word boundary system is built on three key components:

1. **Pattern Strategy Selection**
   - Simple patterns use direct boundary checking
   - Regex patterns handle boundaries in two ways:
     - Automatically inject \b if pattern lacks boundary tokens
     - Post-filter matches using the same boundary logic as simple patterns
   - Cached strategies preserve boundary settings

2. **Boundary Detection**
   - Unicode-aware character classification
   - Special handling:
     - Underscores always join words in identifiers
     - Hyphens join or separate based on mode
     - Dash-like punctuation (em-dash, etc.) can be configured
   - Optimized for common code patterns

3. **Pattern Cache Integration**
   - Caches include boundary mode in key
   - Separate cache entries for different modes
   - Maintains performance characteristics

## Smart Hyphen Handling

One of the most interesting challenges we faced was handling hyphens and underscores in different contexts. Consider these scenarios:

```rust
// In code (hyphens join words):
let test-case = "example";  // "test-case" is one token
my-function-name();         // "my-function-name" is one token

// In natural text (hyphens separate words):
"hello-world café"         // "hello" and "world" are separate
"user-friendly interface"  // "user" and "friendly" are separate

// Underscores always join in code:
let test_case = "example";  // "test_case" is always one token
fn my_function_name();      // "my_function_name" is always one token
```

We solved this with a flexible hyphen handling system, while treating underscores as special:

```rust
#[derive(Debug, Clone, Copy)]
pub enum HyphenHandling {
    /// Treat hyphens as word boundaries (natural text mode)
    Boundary,
    /// Treat hyphens as joining characters (code identifier mode)
    Joining,
}
```

Each pattern can specify its preferred hyphen handling:

```rust
let config = SearchConfig {
    pattern_definitions: vec![PatternDefinition {
        text: "test".to_string(),
        is_regex: false,
        boundary_mode: WordBoundaryMode::WholeWords,
        hyphen_handling: HyphenHandling::Joining, // for code search
    }],
    // ...
};
```

The default is `HyphenHandling::Joining`, optimized for code search:
- In code: "test" won't match inside "test-case" (hyphen joins in Joining mode)
- In text: Use `HyphenHandling::Boundary` to match "hello" in "hello-world"
- Special case: Underscores ALWAYS join words in identifiers, regardless of mode
  - "test" never matches in "test_case" (underscore always joins)
  - This preserves identifier unity in all modes

This flexibility allows RustScout to handle both code and text search patterns appropriately, while ensuring consistent behavior for code identifiers.

## Performance Impact

Our benchmarks show minimal overhead:
- Simple pattern search: ~1% overhead
- Regex pattern search: No measurable overhead
- Memory usage: No significant increase
- Cache efficiency: Maintained with boundary modes

## Unicode Support

One of the key improvements in our word boundary implementation is comprehensive Unicode support. RustScout now correctly handles word boundaries across different writing systems and scripts:

### Multi-Script Support
```bash
# Latin script with diacritics
rustscout search -p "café" -w     # Matches "café" but not "café-bar"

# Cyrillic script
rustscout search -p "привет" -w   # Matches "привет" but not "приветствие"

# CJK characters
rustscout search -p "你好" -w     # Matches "你好" but not "你好吗"

# Korean Hangul
rustscout search -p "안녕" -w     # Matches "안녕" but not "안녕하세요"

# Mixed-script identifiers
rustscout search -p "hello_世界" -w  # Matches full identifier only
rustscout search -p "test_café_안녕" -w  # Handles complex mixed-script cases
```

The implementation uses Unicode properties to correctly identify:
- Letters and numbers across all scripts
- Non-spacing marks (combining diacritics)
- Spacing marks
- Enclosing marks
- Script-specific punctuation

### Smart Script Bridging

RustScout implements intelligent script bridging for identifiers:
- Underscores always act as joiners in identifiers
- Special handling when bridging different scripts (e.g., ASCII to non-ASCII)
- Preserves identifier unity across script boundaries

### Comprehensive Character Support

The system handles a wide range of characters:
- All Unicode hyphens and dashes (U+2010 through U+2015)
- Mathematical symbols (∑, β, etc.) with special identifier rules
- Technical symbols with context-aware joining
- Right-to-left scripts (Hebrew שלום, Arabic مرحبا)
- Emoji (including skin tones and combined sequences like 👨‍👩‍👧‍👦)
- Common identifier characters (@, #, $)
- String/char literal markers (' and `)

### Edge Cases and Special Handling

RustScout correctly handles complex scenarios:
```bash
# Mathematical identifiers
rustscout search -p "∑" -w     # Smart handling in ∑_total

# Mixed scripts with hyphens
rustscout search -p "café-bar" -w  # Respects hyphen mode

# Complex emoji
rustscout search -p "👨‍👩‍👧‍👦" -w  # Handles combined sequences

# Technical identifiers
rustscout search -p "test_café" -w  # Preserves identifier unity
```

This comprehensive Unicode support ensures accurate search results in:
- International codebases
- Scientific/mathematical code
- Modern web applications using emoji
- Mixed-language documentation
- Technical documentation with special symbols

## Real-World Benefits

The word boundary feature significantly improves search precision:

### Finding TODOs
```bash
# Without boundaries: Matches "TODOLIST", "TODOS", etc.
rustscout search -p TODO
Found 360401 matches in 72 files

# With boundaries: Only matches standalone "TODO"
rustscout search -p TODO -w true
Found 33 matches in 3 files
```

### Function Search
```bash
# Without boundaries: Matches "address", "padding", etc.
rustscout search -p add
Found 1205 matches

# With boundaries: Only matches "add" function/variable
rustscout search -p add -w true
Found 47 matches
```

## Development Process

This feature showcases the power of modern software development practices. The implementation was developed through an AI-assisted process, emphasizing:
- Clean, maintainable code
- Comprehensive test coverage
- Clear documentation
- Performance optimization

The result is a robust, production-ready feature that integrates seamlessly with existing functionality.

## What's Next?

We continue to improve RustScout's search capabilities:
- Additional boundary mode options
- Custom boundary character sets
- Performance optimizations for large codebases
- Enhanced Unicode support

## Try It Out

Update to the latest version to use word boundary search:

```bash
cargo install rustscout-cli
```

## Contributing

We welcome contributions to make RustScout even better:
- Feature suggestions
- Performance improvements
- Bug reports
- Documentation enhancements

Visit our [GitHub repository](https://github.com/willibrandon/rustscout) to get involved.

## Acknowledgments

Special thanks to:
- Our users for valuable feature requests
- The Rust community for excellent tools and crates
- Contributors who help improve RustScout

---

*This post is part of our series on building powerful code search capabilities. Follow us for more updates!* 