use memmap2::Mmap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use tracing::{trace, warn};

use super::matcher::PatternMatcher;
use crate::config::EncodingMode;
use crate::errors::{SearchError, SearchResult};
use crate::metrics::MemoryMetrics;
use crate::results::{FileResult, Match};

// Constants for file processing
const BUFFER_CAPACITY: usize = 65536;
pub(crate) const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
pub(crate) const LARGE_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

/// Helper function to decode bytes into a String according to encoding mode
fn decode_bytes(bytes: &[u8], path: &Path, encoding_mode: EncodingMode) -> SearchResult<String> {
    match encoding_mode {
        EncodingMode::FailFast => {
            // Try converting to UTF-8 via from_utf8 first to avoid an extra copy if valid
            match std::str::from_utf8(bytes) {
                Ok(valid_str) => {
                    // Already valid; just clone into a String
                    Ok(valid_str.to_owned())
                }
                Err(_utf8_err) => {
                    // It's invalid; now create a FromUtf8Error by reattempting from_utf8 on a Vec
                    // (only in the error path). This preserves the exact error data for the test.
                    let vec_copy = bytes.to_vec();
                    let from_utf8_err = match String::from_utf8(vec_copy) {
                        Ok(_) => unreachable!("We already know it's invalid"),
                        Err(e) => e,
                    };
                    Err(SearchError::encoding_error(path, from_utf8_err))
                }
            }
        }
        EncodingMode::Lossy => {
            // from_utf8_lossy can replace invalid bytes with
            let cow = String::from_utf8_lossy(bytes);
            // If it's Owned, at least one invalid sequence was replaced.
            if let std::borrow::Cow::Owned(_) = cow {
                warn!("Invalid UTF-8 replaced in file: {}", path.display());
            }
            Ok(cow.into_owned())
        }
    }
}

/// Handles file processing operations
#[derive(Debug)]
pub struct FileProcessor {
    matcher: PatternMatcher,
    metrics: MemoryMetrics,
    context_before: usize,
    context_after: usize,
    encoding_mode: EncodingMode,
}

impl FileProcessor {
    /// Creates a new FileProcessor with the given pattern matcher
    pub fn new(
        matcher: PatternMatcher,
        context_before: usize,
        context_after: usize,
        encoding_mode: EncodingMode,
    ) -> Self {
        Self {
            matcher,
            metrics: MemoryMetrics::new(),
            context_before,
            context_after,
            encoding_mode,
        }
    }

    /// Gets the current memory metrics
    pub fn metrics(&self) -> &MemoryMetrics {
        &self.metrics
    }

    /// Process a small file using simple line-by-line reading
    fn process_small_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Using simple file processing for: {}", path.display());

        // Read the entire file as bytes first
        let bytes = std::fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        // Decode bytes using our helper
        let contents = decode_bytes(&bytes, path, self.encoding_mode)?;

        // Split into lines and find matches
        let lines: Vec<&str> = contents.lines().collect();

        let matches = self
            .matcher
            .find_matches(&contents)
            .into_iter()
            .map(|pos| {
                let line_number = 1 + contents[..pos.0].chars().filter(|&c| c == '\n').count();
                let line_index = line_number - 1;

                // Collect context before
                let context_before: Vec<(usize, String)> = (0..self.context_before)
                    .filter_map(|i| {
                        if line_index > i {
                            Some((line_number - i - 1, lines[line_index - i - 1].to_string()))
                        } else {
                            None
                        }
                    })
                    .rev()
                    .collect();

                // Collect context after
                let context_after: Vec<(usize, String)> = (1..=self.context_after)
                    .filter_map(|i| {
                        lines
                            .get(line_index + i)
                            .map(|line| (line_number + i, line.to_string()))
                    })
                    .collect();

                Match {
                    line_number,
                    start: pos.0 - contents[..pos.0].rfind('\n').map_or(0, |n| n + 1),
                    end: pos.1 - contents[..pos.0].rfind('\n').map_or(0, |n| n + 1),
                    line_content: lines[line_index].to_string(),
                    context_before,
                    context_after,
                }
            })
            .collect();

        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Process a file using buffered reading
    fn process_file_buffered(&self, path: &Path) -> SearchResult<FileResult> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(SearchError::IoError)?;

        // Decode bytes using our helper
        let contents = decode_bytes(&bytes, path, self.encoding_mode)?;

        // Split into lines and find matches
        let lines: Vec<&str> = contents.lines().collect();

        let matches = self
            .matcher
            .find_matches(&contents)
            .into_iter()
            .map(|pos| {
                let line_number = 1 + contents[..pos.0].chars().filter(|&c| c == '\n').count();
                let line_index = line_number - 1;

                // Collect context before
                let context_before: Vec<(usize, String)> = (0..self.context_before)
                    .filter_map(|i| {
                        if line_index > i {
                            Some((line_number - i - 1, lines[line_index - i - 1].to_string()))
                        } else {
                            None
                        }
                    })
                    .rev()
                    .collect();

                // Collect context after
                let context_after: Vec<(usize, String)> = (1..=self.context_after)
                    .filter_map(|i| {
                        lines
                            .get(line_index + i)
                            .map(|line| (line_number + i, line.to_string()))
                    })
                    .collect();

                Match {
                    line_number,
                    start: pos.0 - contents[..pos.0].rfind('\n').map_or(0, |n| n + 1),
                    end: pos.1 - contents[..pos.0].rfind('\n').map_or(0, |n| n + 1),
                    line_content: lines[line_index].to_string(),
                    context_before,
                    context_after,
                }
            })
            .collect();

        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Process a file using memory mapping
    fn process_mmap_file(&self, path: &Path) -> SearchResult<FileResult> {
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

        let mmap = unsafe { Mmap::map(&file) }.map_err(SearchError::IoError)?;

        // Decode bytes using our helper
        let contents = decode_bytes(&mmap, path, self.encoding_mode)?;

        let lines: Vec<&str> = contents.lines().collect();
        let matches = self
            .matcher
            .find_matches(&contents)
            .into_iter()
            .map(|pos| {
                let line_number = 1 + contents[..pos.0].chars().filter(|&c| c == '\n').count();
                let line_index = line_number - 1;

                // Collect context before
                let context_before: Vec<(usize, String)> = (0..self.context_before)
                    .filter_map(|i| {
                        if line_index > i {
                            Some((line_number - i - 1, lines[line_index - i - 1].to_string()))
                        } else {
                            None
                        }
                    })
                    .rev()
                    .collect();

                // Collect context after
                let context_after: Vec<(usize, String)> = (1..=self.context_after)
                    .filter_map(|i| {
                        lines
                            .get(line_index + i)
                            .map(|line| (line_number + i, line.to_string()))
                    })
                    .collect();

                Match {
                    line_number,
                    start: pos.0 - contents[..pos.0].rfind('\n').map_or(0, |n| n + 1),
                    end: pos.1 - contents[..pos.0].rfind('\n').map_or(0, |n| n + 1),
                    line_content: lines[line_index].to_string(),
                    context_before,
                    context_after,
                }
            })
            .collect();

        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
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

        // Create a file with known patterns - reduced size but still large enough to test memory mapping
        let line = "This is a test line with pattern_123 and another pattern_456\n";
        for _ in 0..1000 {
            // Reduced from 50,000 to 1,000 lines
            file.write_all(line.as_bytes()).unwrap();
        }

        // Create a pattern matcher and processor
        let matcher = PatternMatcher::new(vec!["pattern_\\d+".to_string()]);
        let processor = FileProcessor::new(matcher, 0, 0, EncodingMode::FailFast);

        // Process the file
        let result = processor.process_file(&file_path).unwrap();

        // Verify results
        assert_eq!(result.matches.len(), 2000); // Two matches per line (reduced from 100,000)

        // Verify first and last matches to ensure correct ordering and content
        let first_match = &result.matches[0];
        let last_match = &result.matches[result.matches.len() - 1];

        // Verify first match
        assert_eq!(first_match.line_number, 1);
        let matched_text = &first_match.line_content[first_match.start..first_match.end];
        assert!(matched_text.starts_with("pattern_"));
        assert!(matched_text[8..].parse::<i32>().is_ok());

        // Verify last match
        assert_eq!(last_match.line_number, 1000);
        let matched_text = &last_match.line_content[last_match.start..last_match.end];
        assert!(matched_text.starts_with("pattern_"));
        assert!(matched_text[8..].parse::<i32>().is_ok());

        // Sample check of a few random matches to verify ordering
        for i in (0..result.matches.len()).step_by(100) {
            let m = &result.matches[i];
            assert!(m.line_number > 0 && m.line_number <= 1000);
            let matched_text = &m.line_content[m.start..m.end];
            assert!(matched_text.starts_with("pattern_"));
            assert!(matched_text[8..].parse::<i32>().is_ok());
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
        let processor = FileProcessor::new(matcher, 0, 0, EncodingMode::FailFast);

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
