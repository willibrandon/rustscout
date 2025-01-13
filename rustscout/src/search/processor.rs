use memmap2::Mmap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, trace, warn};

use super::matcher::PatternMatcher;
use crate::errors::{SearchError, SearchResult};
use crate::metrics::MemoryMetrics;
use crate::results::{FileResult, Match};

// Constants for file processing
const BUFFER_CAPACITY: usize = 65536; // Increased from 8192 for better I/O performance
pub(crate) const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
pub(crate) const LARGE_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB
const MAX_LINES_WITHOUT_MATCH: usize = 100; // Stop reading after this many lines without a match

/// A single ring buffer slot with reusable `String`.
/// We store:
///  - line_number: which file line this slot corresponds to
///  - text: the actual line text (reused for the lifetime of the buffer slot)
#[derive(Debug)]
struct RingSlot {
    line_number: usize,
    text: String,
}

/// Handles file processing operations
#[derive(Debug)]
pub struct FileProcessor {
    matcher: PatternMatcher,
    metrics: MemoryMetrics,
    context_before: usize,
    context_after: usize,
}

impl FileProcessor {
    /// Creates a new FileProcessor with the given pattern matcher
    pub fn new(matcher: PatternMatcher, context_before: usize, context_after: usize) -> Self {
        Self {
            matcher,
            metrics: MemoryMetrics::new(),
            context_before,
            context_after,
        }
    }

    /// Gets the current memory metrics
    pub fn metrics(&self) -> &MemoryMetrics {
        &self.metrics
    }

    /// Processes a file and returns any matches found
    pub fn process_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Processing file: {}", path.display());

        // Choose processing strategy based on file size
        match path.metadata() {
            Ok(metadata) => {
                let size = metadata.len();
                self.metrics.record_file_processing(size);

                if size < SMALL_FILE_THRESHOLD {
                    self.process_small_file(path)
                } else if size >= LARGE_FILE_THRESHOLD {
                    self.process_mmap_file(path)
                } else {
                    self.process_file_buffered(path)
                }
            }
            Err(e) => {
                warn!("Failed to get metadata for {}: {}", path.display(), e);
                self.process_file_buffered(path)
            }
        }
    }

    /// Collect "before context" lines from the ring buffer.
    ///
    /// buffer: the ring buffer of lines
    /// ring_size: total capacity of the ring buffer
    /// current_pos: index of the *current* line in the buffer
    /// current_line_num: the line number for the *current* line
    /// count: how many lines of context to retrieve
    ///
    /// Returns up to `count` lines `(line_num, text)` in ascending line-number order.
    fn collect_context_lines(
        buffer: &[RingSlot],
        ring_size: usize,
        current_pos: usize,
        current_line_num: usize,
        count: usize,
    ) -> Vec<(usize, String)> {
        // We'll store them in reverse, then flip at the end
        // so we get ascending line numbers in the final result.
        let mut result_rev = Vec::with_capacity(count);

        // If there's no valid line at the current position (line_number == 0),
        // we can't proceed, so just return empty.
        if buffer[current_pos].line_number == 0 {
            return Vec::new();
        }

        // Each offset is how many lines "back" we go.
        for offset in 1..=count {
            // If subtracting offset goes below line #1, stop (file boundary).
            let wanted_line_num = match current_line_num.checked_sub(offset) {
                Some(n) if n >= 1 => n,
                _ => break,
            };

            // Because we're storing lines sequentially in the ring,
            // we can directly compute the index for the wanted line:
            let index = (ring_size + current_pos - offset) % ring_size;

            // If the ring has overwritten old lines, they may no longer match
            // the wanted_line_num. So we verify:
            if buffer[index].line_number == wanted_line_num {
                // Clone out (line_num, text) for final result
                result_rev.push((wanted_line_num, buffer[index].text.clone()));
            } else {
                // If it's not the line we want, it means it's overwritten or
                // we had a ring size too small for how far back we want to go.
                // Just stop collecting—older lines are no longer valid.
                break;
            }
        }

        // Reverse so final order is ascending by line number
        result_rev.reverse();
        result_rev
    }

    /// Process a small file using simple line-by-line reading
    fn process_small_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Using simple file processing for: {}", path.display());
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        let mut reader = BufReader::new(file);
        let mut matches = Vec::new();
        let mut line_number = 0;
        let mut last_match = 0;

        // Ring buffer for context lines
        let ring_size = self.context_before + 1;
        let mut ring_buffer: Vec<RingSlot> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            ring_buffer.push(RingSlot {
                line_number: 0,
                text: String::new(),
            });
        }

        let mut ring_pos = 0;
        let mut pending_match: Option<Match> = None;
        let mut context_after_count = 0;

        let mut line_buf = String::new();

        // Process each line
        loop {
            line_buf.clear();
            let bytes_read = reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                break; // End of file
            }
            line_number += 1;

            // Store current line in ring buffer (reuse the allocated String)
            {
                let slot = &mut ring_buffer[ring_pos];
                slot.line_number = line_number;
                slot.text.clear();
                slot.text.push_str(&line_buf);
            }

            // Save the ring buffer position that just got updated
            let current_pos = ring_pos;
            // Advance for next line
            ring_pos = (ring_pos + 1) % ring_size;

            // Handle any pending match's "after context"
            if let Some(mut m) = pending_match.take() {
                if context_after_count < self.context_after {
                    m.context_after.push((line_number, line_buf.clone()));
                    context_after_count += 1;
                    pending_match = Some(m);
                } else {
                    matches.push(m);
                }
            }

            // Check for matches in the current line
            let line_matches = self.matcher.find_matches(&line_buf);
            if !line_matches.is_empty() {
                // Because multiple matches in the same line share the same context,
                // we only collect "before context" once for that line
                let before_context = Self::collect_context_lines(
                    &ring_buffer,
                    ring_size,
                    current_pos,
                    line_number,
                    self.context_before,
                );

                for (start, end) in line_matches {
                    // If we had a pending match waiting for after-context,
                    // we must finalize it before adding a new one for this line
                    if let Some(m) = pending_match.take() {
                        matches.push(m);
                    }

                    let m = Match {
                        line_number,
                        line_content: line_buf.clone(),
                        start,
                        end,
                        context_before: before_context.clone(),
                        context_after: Vec::new(),
                    };

                    // If we do want after-context, store it for future lines
                    if self.context_after > 0 {
                        pending_match = Some(m);
                        context_after_count = 0;
                    } else {
                        // No after-context—store right away
                        matches.push(m);
                    }
                }

                last_match = line_number;
            }

            if line_number > MAX_LINES_WITHOUT_MATCH && last_match == 0 {
                debug!(
                    "No matches in first {} lines, skipping rest of file",
                    MAX_LINES_WITHOUT_MATCH
                );
                break;
            }
        }

        // If we exit the loop with a pending match, push it now
        if let Some(m) = pending_match {
            matches.push(m);
        }

        debug!("Found {} matches in file {}", matches.len(), path.display());
        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Process a file using buffered reading
    fn process_file_buffered(&self, path: &Path) -> SearchResult<FileResult> {
        let file = File::open(path)?;
        let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
        let mut line_number: usize = 0;
        let mut matches = Vec::new();

        // Ring buffer for *before* context
        // We only need `context_before + 1` slots:
        //   - up to `context_before` lines + 1 slot for the current line
        let ring_size = self.context_before + 1;
        let mut ring_buffer: Vec<RingSlot> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            ring_buffer.push(RingSlot {
                line_number: 0,
                text: String::new(),
            });
        }

        let mut ring_pos = 0;

        // Variables for handling "after context"
        let mut pending_match: Option<Match> = None;
        let mut context_after_count = 0;

        let mut line_buf = String::new();
        let mut lines_without_match = 0;

        // Read lines in a loop
        loop {
            line_buf.clear();
            let bytes_read = reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                break; // End of file
            }
            line_number += 1;

            // Store current line in ring buffer (reuse the allocated String)
            {
                let slot = &mut ring_buffer[ring_pos];
                slot.line_number = line_number;
                slot.text.clear();
                slot.text.push_str(&line_buf);
            }

            // Save the ring buffer position that just got updated
            let current_pos = ring_pos;
            // Advance for next line
            ring_pos = (ring_pos + 1) % ring_size;

            // 1) Handle any pending match's "after context"
            if let Some(mut m) = pending_match.take() {
                if context_after_count < self.context_after {
                    // Still collecting after-context lines
                    m.context_after.push((line_number, line_buf.clone()));
                    context_after_count += 1;
                    pending_match = Some(m);
                } else {
                    // We've satisfied the after-context requirement, store match
                    matches.push(m);
                }
            }

            // 2) Check for new matches in the *current* line
            let line_matches = self.matcher.find_matches(&line_buf);
            if !line_matches.is_empty() {
                lines_without_match = 0;

                // Because multiple matches in the same line share the same context,
                // we only collect "before context" once for that line
                let before_context = Self::collect_context_lines(
                    &ring_buffer,
                    ring_size,
                    current_pos,
                    line_number,
                    self.context_before,
                );

                for (start, end) in line_matches {
                    // If we had a pending match waiting for after-context,
                    // we must finalize it before adding a new one for this line
                    if let Some(m) = pending_match.take() {
                        matches.push(m);
                    }

                    let m = Match {
                        line_number,
                        line_content: line_buf.clone(),
                        start,
                        end,
                        context_before: before_context.clone(),
                        context_after: Vec::new(),
                    };

                    // If we do want after-context, store it for future lines
                    if self.context_after > 0 {
                        pending_match = Some(m);
                        context_after_count = 0;
                    } else {
                        // No after-context—store right away
                        matches.push(m);
                    }
                }
            } else {
                lines_without_match += 1;
                if lines_without_match >= MAX_LINES_WITHOUT_MATCH {
                    debug!(
                        "No matches found in {} consecutive lines, stopping early",
                        MAX_LINES_WITHOUT_MATCH
                    );
                    break;
                }
            }
        }

        // If we exit the loop with a pending match, push it now
        if let Some(m) = pending_match {
            matches.push(m);
        }

        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Process a file using memory mapping
    fn process_mmap_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!(
            "Using memory-mapped file processing for: {}",
            path.display()
        );
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        let mmap = unsafe { Mmap::map(&file)? };
        let mut matches = Vec::new();
        let mut line_number = 0;
        let mut last_match = 0;

        // Ring buffer for context lines
        let ring_size = self.context_before + 1;
        let mut ring_buffer: Vec<RingSlot> = Vec::with_capacity(ring_size);
        for _ in 0..ring_size {
            ring_buffer.push(RingSlot {
                line_number: 0,
                text: String::new(),
            });
        }

        let mut ring_pos = 0;
        let mut pending_match: Option<Match> = None;
        let mut context_after_count = 0;

        // Process each line
        for line in mmap.split(|&b| b == b'\n') {
            line_number += 1;

            // Convert bytes to string, skipping invalid UTF-8
            let line_str = String::from_utf8_lossy(line);

            // Store current line in ring buffer (reuse the allocated String)
            {
                let slot = &mut ring_buffer[ring_pos];
                slot.line_number = line_number;
                slot.text.clear();
                slot.text.push_str(&line_str);
            }

            // Save the ring buffer position that just got updated
            let current_pos = ring_pos;
            // Advance for next line
            ring_pos = (ring_pos + 1) % ring_size;

            // Handle any pending match's "after context"
            if let Some(mut m) = pending_match.take() {
                if context_after_count < self.context_after {
                    m.context_after.push((line_number, line_str.to_string()));
                    context_after_count += 1;
                    pending_match = Some(m);
                } else {
                    matches.push(m);
                }
            }

            // Check for matches in the current line
            let line_matches = self.matcher.find_matches(&line_str);
            if !line_matches.is_empty() {
                // Because multiple matches in the same line share the same context,
                // we only collect "before context" once for that line
                let before_context = Self::collect_context_lines(
                    &ring_buffer,
                    ring_size,
                    current_pos,
                    line_number,
                    self.context_before,
                );

                for (start, end) in line_matches {
                    // If we had a pending match waiting for after-context,
                    // we must finalize it before adding a new one for this line
                    if let Some(m) = pending_match.take() {
                        matches.push(m);
                    }

                    let m = Match {
                        line_number,
                        line_content: line_str.to_string(),
                        start,
                        end,
                        context_before: before_context.clone(),
                        context_after: Vec::new(),
                    };

                    // If we do want after-context, store it for future lines
                    if self.context_after > 0 {
                        pending_match = Some(m);
                        context_after_count = 0;
                    } else {
                        // No after-context—store right away
                        matches.push(m);
                    }
                }

                last_match = line_number;
            }

            if line_number > MAX_LINES_WITHOUT_MATCH && last_match == 0 {
                debug!(
                    "No matches in first {} lines, skipping rest of file",
                    MAX_LINES_WITHOUT_MATCH
                );
                break;
            }
        }

        // If we exit the loop with a pending match, push it now
        if let Some(m) = pending_match {
            matches.push(m);
        }

        debug!("Found {} matches in file {}", matches.len(), path.display());
        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parallel_pattern_matching() {
        // Create a temporary directory and file
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("large_test.txt");
        let mut file = File::create(&file_path).unwrap();

        // Create a large file with known patterns
        let line = "This is a test line with pattern_123 and another pattern_456\n";
        for _ in 0..50_000 {
            // Creates a file > 10MB to trigger memory mapping
            file.write_all(line.as_bytes()).unwrap();
        }

        // Create a pattern matcher and processor
        let matcher = PatternMatcher::new(vec!["pattern_\\d+".to_string()]);
        let processor = FileProcessor::new(matcher, 0, 0);

        // Process the file
        let result = processor.process_file(&file_path).unwrap();

        // Verify results
        assert_eq!(result.matches.len(), 100_000); // Two matches per line

        // Verify matches are correctly ordered
        let mut prev_line = 0;
        let mut prev_start = 0;
        for match_result in &result.matches {
            if match_result.line_number == prev_line {
                // Within the same line, start position should increase
                assert!(
                    match_result.start > prev_start,
                    "Match positions within line {} should be increasing: prev={}, current={}",
                    match_result.line_number,
                    prev_start,
                    match_result.start
                );
            } else {
                // New line should be greater than previous line
                assert!(
                    match_result.line_number > prev_line,
                    "Line numbers should be strictly increasing: prev={}, current={}",
                    prev_line,
                    match_result.line_number
                );
            }
            prev_line = match_result.line_number;
            prev_start = match_result.start;

            // Verify match content
            let matched_text = &match_result.line_content[match_result.start..match_result.end];
            assert!(
                matched_text.starts_with("pattern_"),
                "Matched text should start with 'pattern_'"
            );
            assert!(
                matched_text[8..].parse::<i32>().is_ok(),
                "Should end with numbers"
            );
        }
    }

    #[test]
    fn test_chunk_boundary_handling() {
        // Create a temporary directory and file
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("boundary_test.txt");
        let mut file = File::create(&file_path).unwrap();

        // Create content that spans chunk boundaries
        let mut content = String::new();
        for i in 0..2000 {
            content.push_str(&format!("Line {} with pattern_split", i));
            // Add varying line lengths to test boundary handling
            if i % 3 == 0 {
                content.push_str(" extra text to vary line length");
            }
            content.push('\n');
        }
        file.write_all(content.as_bytes()).unwrap();

        // Create a pattern matcher and processor
        let matcher = PatternMatcher::new(vec!["pattern_split".to_string()]);
        let processor = FileProcessor::new(matcher, 0, 0);

        // Process the file
        let result = processor.process_file(&file_path).unwrap();

        // Verify results
        assert_eq!(result.matches.len(), 2000); // One match per line

        // Verify all matches are found and in order
        let mut prev_line = 0;
        for match_result in &result.matches {
            assert!(
                match_result.line_number > prev_line,
                "Line numbers should be strictly increasing"
            );
            assert!(
                match_result.line_content.contains("pattern_split"),
                "Each line should contain the pattern"
            );
            prev_line = match_result.line_number;
        }
    }
}
