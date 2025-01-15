# Word Boundary Search & Advanced File Handling in RustScout

## Introduction

RustScout is a **powerful, high-performance code search tool** that has evolved significantly over the past few releases. In this post, we'll **double down** on two major developments:

1. **Word Boundary Search**: A feature for more precise pattern matching.  
2. **Improved File Handling and UTF-8 Logic**: Including robust `.gitignore`-style ignore patterns, fail-fast and lossy UTF-8 modes, path normalization, and a fix for `.git/index` binary files.

Throughout this journey, we leveraged **AI** in two key ways:
- **ChatGPT**: Provided architectural and design guidance, helping us diagnose layered issues and devise a robust plan.  
- **Claude 3.5 Sonnet**: Generated much of the actual code, ensuring correctness and consistency while saving development time.

The result? A **fully passing test suite**, improved user experience, and a more **intuitive** `.gitignore`-style system for ignoring files like `.git/index` or `invalid.rs`.

---

## Part I: Word Boundary Search

### The Challenge: Precise Pattern Matching

Without word boundaries, searching for "test" might also match "testing" or "contest". Similarly, searching for "TODO" might yield "TODOS" or "TODOLIST." This noise obscures the results you truly want.

### The Solution: Smart Word Boundary Detection

We introduced a flexible system:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBoundaryMode {
    None,
    WholeWords,
}
```

- **`None`**: No boundary checks (the original RustScout behavior).
- **`WholeWords`**: Ensures that matches occur only at the edges of words.

#### Example Usage

```bash
# Find standalone "TODO" comments
rustscout search -p TODO -w true
```
This matches only standalone `TODO` instances, not inside `TODOS` or `TODOLIST`.

### Implementation Details

We handle boundaries in **both simple** and **regex** strategies:
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
                regex.find_iter(text)
                     .map(|m| (m.start(), m.end()))
                     .collect()
            }
        }
    }
}
```

### Smart Hyphen Handling

We also introduced the concept of **hyphen vs. underscore** handling to differentiate code identifier logic from natural text usage:

```rust
#[derive(Debug, Clone, Copy)]
pub enum HyphenHandling {
    Boundary,  // treat hyphens as separators
    Joining,   // treat hyphens as part of the same token
}
```

- **Underscores** always join in code identifiers. 
- Hyphens can be set to **Boundary** (e.g., natural text "hello-world") or **Joining** (typical in code "my-function-name").

### Unicode Support

We fully embraced **Unicode**:
- Properly detect word boundaries across multiple scripts (Latin, Cyrillic, CJK, etc.).
- Handle diacritics, combining marks, and emoji without partial matches.
- Preserve code identifier unity across scripts and special symbols.

**Performance Impact**: Our benchmarks show minimal overhead for this boundary logic; memory usage and speed remain competitive.

### Real-World Benefits

- Pinpoint "add" function calls without matching "address".
- Cleanly handle "TODO" without partial matches.
- Effortlessly search code spanning multiple languages, hyphenated identifiers, or emoji.

---

## Part II: Advanced File Handling & UTF-8 Logic

While word boundary search solved partial match issues, we discovered more **layered** challenges involving:

1. **Ignore Patterns** using `.gitignore`-style semantics.  
2. **Fail-Fast vs. Lossy UTF-8** modes for reading files with questionable encodings.  
3. **Consistent Path Normalization** on Windows (e.g., `\\?\C:\` vs. `C:\`).  
4. **Avoiding Binary Files** like `.git/index` in Git repositories.

### 1. .gitignore-Style Ignore Patterns

#### Challenge

Simple "invalid.rs" patterns wouldn't match "C:\Users...\invalid.rs" in absolute form. Also, single star `*` could accidentally cross directory boundaries.

#### Our Enhancements

- **Filename vs. Glob**: No slash in pattern => match by filename only. Slash present => do a full path glob.  
- **`require_literal_separator = true`**: Ensures `"src/*.rs"` won't match `"src/nested/foo.rs"`, aligning with typical `.gitignore` behavior.
- **Relative Path Matching**: Strip the root path to produce a relative path (e.g., `src/foo.rs`), then run the glob.

**Result**: The tool now respects `invalid.rs` for any file named "invalid.rs," or "**/invalid.rs" for nested directories, plus globs like `"src/*.rs"` strictly match only top-level files in `src/`.

### 2. Fail-Fast & Lossy UTF-8 Modes

#### Problem

Some files (like `.rs` in code) must be valid UTF-8 to read properly, but others contain invalid sequences. Or we have a file in a legacy encoding.

#### Solution: Two Approaches

1. **FailFast**:  
   - Attempt `std::str::from_utf8`. If invalid, do a minimal copy to produce a `FromUtf8Error`. Immediately error out.  
   - Perfect for code files where valid UTF-8 is required.

2. **Lossy**:  
   - Use `String::from_utf8_lossy`. Replace invalid sequences with ` `.  
   - Continue searching, only logging a warning if replacements occur.

**Why it Helps**  
- We **stop** early if we rely on valid UTF-8.  
- We **keep** searching in a best-effort manner if we only want partial matches in text.

### 3. Consistent Path Normalization

We discovered subtle Windows differences like `"\\?\C:\Users..."` vs. `"C:\Users..."`. Our fix:

- **`unify_path`** function.  
- Called in both the **error** constructor (`SearchError::encoding_error`) and test's path comparison.  
- Ensures identical wide characters, no hidden suffixes, no mismatched UNC prefixes.

### 4. Handling `.git/index` as Binary

#### The `.git` Surprise

A test called `test_incremental_search_git_strategy` tried reading `.git/index`—a **binary** file—as UTF-8. It triggered `Invalid UTF-8 in file .git/index.`

#### The Fix

- **Ignore** `.git` directory by default (like many code search tools).  
- Or treat `.git/index` as a strictly **binary** file if the incremental strategy needs to parse it at the binary level.  
- This resolved the last failing test.

---

## The Testing Journey

We had a **multi-layered** approach to debugging:

1. **Path mismatch**: We added debug logs revealing the paths were, in fact, identical. The real issue was ignoring `invalid.rs` in the second search.  
2. **Glob crossing**: Using `require_literal_separator = false` was letting `src/*.rs` match subdirectories. We switched to `matches_with(..., require_literal_separator = true)`.  
3. **Binary `.git/index`**: We added `".git", ".git/*", "**/.git/**"` to ignore patterns, ensuring we don't parse Git internals as text.  
4. **FailFast** vs. **Lossy** verified with multiple UTF-8 edge cases—both modes now pass comprehensive tests.

### Key Learnings

1. **Layered Problem Solving**:  
   - We overcame a set of issues in the right order (ignore patterns, glob matching, path handling).  
2. **Best Practices Alignment**:  
   - `.gitignore`-style semantics made ignore patterns more intuitive.  
   - Using `require_literal_separator = true` matched user expectations for `*` vs. `**`.  
3. **Performance**:  
   - Memory mapping is preserved for large files; we skip early on binary or invalid data.  
   - No overhead from boundary mode or advanced ignoring.  
4. **Testing Strategy**:  
   - A comprehensive suite (unit, integration, CLI tests, doc tests) caught subtle errors.  
   - Debug logging was crucial in diagnosing actual vs. perceived mismatches.

---

## Post-Mortem & Confirmations

After unifying path handling, adopting `.gitignore` patterns, adding fail-fast/invalid UTF-8 logic, and ignoring `.git/index`, **all tests now pass**:

- **37** unit tests  
- **6** CLI tests  
- **21** integration tests  
- **Doc tests** (24 ignored, 0 failed)

In particular:

- **Invalid UTF-8** is handled or replaced.  
- **Ignore logic** robustly matches or skips subdirectories.  
- **Paths** are unified on Windows.  
- **.git/index** is no longer read as text.

### Key Enhancements

1. **Ignore Patterns**  
   - No slash → **filename match** ("invalid.rs").  
   - Slash → **full path glob** with `require_literal_separator = true` for `.gitignore`-style behavior.  

2. **Improved `should_ignore`**  
   - Takes `root_path`, produces relative slash path, uses `Pattern::matches_with(..., require_literal_separator = true)`.  

3. **Fail-Fast UTF-8 Mode**  
   - Reads raw bytes, checks valid UTF-8.  
   - If invalid, a minimal copy yields a `FromUtf8Error`.  

4. **Lossy UTF-8 Mode**  
   - `String::from_utf8_lossy` logs a warning, continuing search.  

5. **Consistent Path Normalization**  
   - Shared `unify_path` removes UNC prefixes or case mismatches.  

**Result**:  
- **Better user experience** for `.gitignore`-style patterns.  
- **Correct** handling of memory-mapped, large files.  
- **Fully passing** test suite with fail-fast/lossy modes and proper `.git` ignoring.

---

## The AI Collaboration

A notable aspect of this journey was **how AI assisted** us:

1. **ChatGPT**:  
   - Provided design insights, recommended solutions for layered issues.  
   - Diagnosed hidden path mismatches, suggested debug logging, and guided step-by-step problem solving.

2. **Claude 3.5 Sonnet**:  
   - Took the architectural approach from ChatGPT.  
   - Generated much of the Rust code, implementing advanced glob logic and fail-fast/invalid UTF-8 modes.  
   - Ensured consistent style and correctness.

This synergy of AI tools helped us **rapidly** prototype, refine, and fix deeply nested problems, culminating in a stable, high-performance solution.

---

## Conclusion & Next Steps

RustScout's **word boundary search** and **refined file/UTF-8 handling** combine to deliver:

- **Accurate** partial vs. whole word detection.  
- **Robust** ignoring of `.git` and other special folders.  
- **Fail-Fast** or **Lossy** modes for text reading.  
- **Unified** path handling across operating systems.

Our entire **test suite** is now green, confirming the system is **reliable**, **efficient**, and **easy** to use. We'll continue iterating on performance optimizations, custom boundary sets, and deeper Git integrations.

**Try it out**:

```bash
cargo install rustscout-cli
rustscout search -p "test" -w true
```

**Contributions** are welcome on [GitHub](https://github.com/willibrandon/rustscout). Whether you're adding features, improving performance, or refining documentation, we look forward to your pull requests!

**Thank you** for joining us on this journey—and special thanks to **ChatGPT** (for architectural/design insights) and **Claude 3.5 Sonnet** (for code generation and implementation). Together, we built a more **intuitive** and **robust** RustScout experience for developers worldwide.

---

*This post is part of our series on building powerful code search capabilities. Follow us for more updates!* 