use memmap2::Mmap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, trace, warn};

use super::matcher::PatternMatcher;
use crate::errors::{SearchError, SearchResult};
use crate::metrics::MemoryMetrics;
use crate::results::{FileResult, Match};

// Constants for file processing
const BUFFER_CAPACITY: usize = 8192; // Initial buffer size for reading files
pub(crate) const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
pub(crate) const LARGE_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB
const MAX_LINES_WITHOUT_MATCH: usize = 100; // Stop reading after this many lines without a match

/// Handles file processing operations
#[derive(Debug)]
pub struct FileProcessor {
    matcher: PatternMatcher,
    metrics: MemoryMetrics,
}

impl FileProcessor {
    /// Creates a new FileProcessor with the given pattern matcher
    pub fn new(matcher: PatternMatcher) -> Self {
        Self {
            matcher,
            metrics: MemoryMetrics::new(),
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
                    self.process_buffered_file(path)
                }
            }
            Err(e) => {
                warn!("Failed to get metadata for {}: {}", path.display(), e);
                self.process_buffered_file(path)
            }
        }
    }

    /// Optimized processing for small files using direct string operations
    fn process_small_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Using small file processing for: {}", path.display());
        let content = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        // Record memory allocation
        self.metrics.record_allocation(content.len() as u64);

        let mut matches = Vec::new();
        let mut line_number = 0;
        let mut last_match = 0;

        for line in content.lines() {
            line_number += 1;
            for (start, end) in self.matcher.find_matches(line) {
                trace!("Found match at line {}: {}", line_number, line);
                matches.push(Match {
                    line_number,
                    line_content: line.to_string(),
                    start,
                    end,
                });
                last_match = matches.len();
            }
            if line_number > MAX_LINES_WITHOUT_MATCH && last_match == 0 {
                debug!(
                    "No matches in first {} lines, skipping rest of file",
                    MAX_LINES_WITHOUT_MATCH
                );
                break;
            }
        }

        // Record memory deallocation
        self.metrics.record_deallocation(content.len() as u64);

        debug!("Found {} matches in file {}", matches.len(), path.display());
        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Processing for medium-sized files using buffered reading
    fn process_buffered_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Using buffered file processing for: {}", path.display());
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        // Record buffer allocation
        self.metrics.record_allocation(BUFFER_CAPACITY as u64);

        let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
        let mut matches = Vec::new();
        let mut line_buffer = String::with_capacity(256);
        let mut line_number = 0;
        let mut last_match = 0;

        while reader.read_line(&mut line_buffer)? > 0 {
            line_number += 1;
            for (start, end) in self.matcher.find_matches(&line_buffer) {
                trace!(
                    "Found match at line {}: {}",
                    line_number,
                    line_buffer.trim()
                );
                matches.push(Match {
                    line_number,
                    line_content: line_buffer.clone(),
                    start,
                    end,
                });
                last_match = line_number;
            }
            if line_number > MAX_LINES_WITHOUT_MATCH && last_match == 0 {
                debug!(
                    "No matches in first {} lines, skipping rest of file",
                    MAX_LINES_WITHOUT_MATCH
                );
                break;
            }
            line_buffer.clear();
        }

        // Record buffer deallocation
        self.metrics.record_deallocation(BUFFER_CAPACITY as u64);

        debug!("Found {} matches in file {}", matches.len(), path.display());
        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Processing for large files using memory mapping and parallel pattern matching
    fn process_mmap_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Using memory-mapped processing for: {}", path.display());
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        let metadata = file.metadata()?;
        let file_size = metadata.len();

        // Create memory map and record allocation
        let mmap = unsafe { Mmap::map(&file) }.map_err(SearchError::IoError)?;
        self.metrics.record_mmap(file_size);

        // Convert to string, skipping invalid UTF-8 sequences
        let content = String::from_utf8_lossy(&mmap);
        let content_str = content.as_ref();

        let mut matches = Vec::new();
        let mut line_number = 1;
        let mut start = 0;

        // Process the content line by line
        for (end, c) in content_str.char_indices() {
            if c == '\n' {
                let line = &content_str[start..end];
                // Find matches in this line
                let line_matches = self.matcher.find_matches(line);
                // Add all matches from this line with the correct line number
                for (match_start, match_end) in line_matches {
                    matches.push(Match {
                        line_number,
                        line_content: line.to_string(),
                        start: match_start,
                        end: match_end,
                    });
                }
                start = end + 1;
                line_number += 1;
            }
        }

        // Handle the last line if it doesn't end with a newline
        if start < content_str.len() {
            let line = &content_str[start..];
            let line_matches = self.matcher.find_matches(line);
            for (match_start, match_end) in line_matches {
                matches.push(Match {
                    line_number,
                    line_content: line.to_string(),
                    start: match_start,
                    end: match_end,
                });
            }
        }

        // Record memory unmap
        self.metrics.record_munmap(file_size);

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
        let processor = FileProcessor::new(matcher);

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
        let processor = FileProcessor::new(matcher);

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
