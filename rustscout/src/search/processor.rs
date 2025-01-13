use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, trace};

use super::matcher::PatternMatcher;
use crate::errors::{SearchError, SearchResult};
use crate::results::{FileResult, Match};

// Constants for file processing
const BUFFER_CAPACITY: usize = 8192; // Initial buffer size for reading files
const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
const MAX_LINES_WITHOUT_MATCH: usize = 100; // Stop reading after this many lines without a match

/// Handles file processing operations
#[derive(Debug)]
pub struct FileProcessor {
    matcher: PatternMatcher,
}

impl FileProcessor {
    /// Creates a new FileProcessor with the given pattern matcher
    pub fn new(matcher: PatternMatcher) -> Self {
        Self { matcher }
    }

    /// Processes a file and returns any matches found
    pub fn process_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Processing file: {}", path.display());

        // Check if we should use optimized processing for small files
        match path.metadata() {
            Ok(metadata) if metadata.len() < SMALL_FILE_THRESHOLD => self.process_small_file(path),
            _ => self.process_large_file(path),
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

        debug!("Found {} matches in file {}", matches.len(), path.display());
        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }

    /// Processing for large files using buffered reading
    fn process_large_file(&self, path: &Path) -> SearchResult<FileResult> {
        trace!("Using large file processing for: {}", path.display());
        let file = File::open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
            std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
            _ => SearchError::IoError(e),
        })?;

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

        debug!("Found {} matches in file {}", matches.len(), path.display());
        Ok(FileResult {
            path: path.to_path_buf(),
            matches,
        })
    }
}
