# Context Lines in RustScout: Better Search Results with Surrounding Code

One of the most requested features for RustScout has been the ability to show context lines around matches. Today, we're excited to announce that this feature is now available in RustScout!

## The Problem

When searching through code, seeing just the matching line often isn't enough. You need to understand the context around the match - the function it's in, the loop condition above it, or the comment that explains what's happening. Without context, you might need to open the file and scroll around to understand what's going on.

## The Solution

RustScout now supports showing context lines before and after each match. You can specify:
- How many lines to show before each match (`--context-before` or `-B`)
- How many lines to show after each match (`--context-after` or `-A`)
- Or use `--context` or `-C` to show the same number of lines before and after

For example:
```bash
# Show 2 lines before and after each match
rustscout --context 2 "TODO"

# Show 3 lines before each match
rustscout -B 3 "TODO"

# Show 2 lines after each match
rustscout -A 2 "TODO"
```

## Key Features

1. **Memory Efficient**: Uses a ring buffer to store only the most recent N lines, ensuring O(1) memory overhead regardless of file size.

2. **Smart Line Collection**: Handles file boundaries gracefully:
   - No context lines before line 1
   - Correct handling of matches near the start/end of files
   - Proper handling of overlapping contexts

3. **Beautiful Output**: Context lines are clearly formatted:
   - Line numbers preserved for all context lines
   - Clear separation between matches
   - Overlapping contexts merged for readability

## Implementation Details

The context line feature is implemented using a ring buffer that stores the most recent N lines of the file being processed. This approach has several advantages:

1. **Constant Memory Usage**: The ring buffer size is fixed based on the maximum context requested, not the file size.

2. **Efficient Line Collection**: When a match is found:
   - We already have the previous N lines in the buffer
   - We can collect following lines as we continue processing
   - Line numbers are preserved for accurate context

3. **Smart Boundary Handling**:
   - Start of file: Only shows available lines before match
   - End of file: Collects remaining lines after match
   - Overlapping matches: Merges contexts efficiently

The implementation carefully handles edge cases:
- Empty files
- Matches at file boundaries
- Consecutive matches with overlapping context
- Files shorter than the requested context

## Performance Impact

Our implementation of context lines has been carefully optimized to maintain RustScout's high performance. Recent benchmarks show that the addition of this feature has not introduced any performance regressions. In fact, the optimizations made during implementation have improved overall performance:

- Base search operations are 10-11% faster for simple patterns
- Regex pattern matching is 9-10% faster
- Large file processing (10MB-100MB) shows 5-8% improvement across different thread counts

The context line feature itself shows excellent performance characteristics:
- No measurable overhead when running without context (`-B0 -A0`)
- Consistent performance with varying context sizes (2, 5, or more lines)
- Efficient handling of both before and after context
- Memory usage remains O(1) thanks to the ring buffer implementation

These improvements come from careful optimizations:
1. Reusing string allocations in the ring buffer
2. Direct indexing for context line collection
3. Smart handling of overlapping contexts
4. Increased I/O buffer size for better throughput

## Future Improvements

We're considering several enhancements:
1. Syntax highlighting for context lines
2. Collapsible context in terminal output
3. Configuration for context line formatting

## Feedback Welcome!

We'd love to hear how you're using the context lines feature and what improvements you'd like to see. Please file issues or contribute to the discussion on GitHub!

## Try It Out

Update to the latest version of RustScout to try out context lines:

```bash
cargo install rustscout
```

Then start exploring your code with more context! 