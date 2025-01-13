use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use crate::config::Config;
use crate::filters::should_include_file;
use crate::results::{FileResult, Match, SearchResult};

// Thresholds for optimization strategies
const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
const SIMPLE_PATTERN_THRESHOLD: usize = 32; // Pattern length for simple search
const MIN_CHUNK_SIZE: usize = 16; // Minimum files per chunk to reduce overhead
const MAX_CHUNK_SIZE: usize = 256; // Maximum files per chunk to ensure good load balancing
const BUFFER_CAPACITY: usize = 8192; // Initial buffer size for reading files

/// Determines if a pattern is "simple" enough for optimized literal search
fn is_simple_pattern(pattern: &str) -> bool {
    pattern.len() < SIMPLE_PATTERN_THRESHOLD
        && !pattern.contains(['*', '+', '?', '[', ']', '(', ')', '|', '^', '$', '.', '\\'])
}

/// Fast path for searching small files with simple patterns
fn search_file_simple(path: &Path, pattern: &str) -> io::Result<FileResult> {
    let content = fs::read_to_string(path)?;
    let mut matches = Vec::new();
    let mut line_number = 0;
    let mut last_match = 0;

    for line in content.lines() {
        line_number += 1;
        if let Some(index) = line.find(pattern) {
            matches.push(Match {
                line_number,
                line_content: line.to_string(),
                start: index,
                end: index + pattern.len(),
            });
            last_match = matches.len();
        }
        // Early exit if no matches found in first N lines
        if line_number > 100 && last_match == 0 {
            break;
        }
    }

    Ok(FileResult {
        path: path.to_path_buf(),
        matches,
    })
}

/// Regular path for searching files with regex patterns
fn search_file_regex(path: &Path, regex: &Regex) -> io::Result<FileResult> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
    let mut matches = Vec::new();
    let mut line_buffer = String::with_capacity(256);
    let mut line_number = 0;
    let mut last_match = 0;

    while reader.read_line(&mut line_buffer)? > 0 {
        line_number += 1;
        if regex.is_match(&line_buffer) {
            for capture in regex.find_iter(&line_buffer) {
                matches.push(Match {
                    line_number,
                    line_content: line_buffer.clone(),
                    start: capture.start(),
                    end: capture.end(),
                });
                last_match = line_number;
            }
        }
        // Early exit if no matches found in first N lines
        if line_number > 100 && last_match == 0 {
            break;
        }
        line_buffer.clear();
    }

    Ok(FileResult {
        path: path.to_path_buf(),
        matches,
    })
}

pub fn search(config: &Config) -> io::Result<SearchResult> {
    if config.pattern.is_empty() {
        return Ok(SearchResult::new());
    }

    // Determine search strategy
    let is_simple = is_simple_pattern(&config.pattern);
    let regex = if !is_simple {
        Some(
            Regex::new(&config.pattern)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
        )
    } else {
        None
    };

    let mut builder = WalkBuilder::new(&config.root_path);
    builder.hidden(true).standard_filters(false);

    for pattern in &config.ignore_patterns {
        builder.add_ignore(pattern);
    }

    let walker = builder.build();

    // Group files by size for optimized processing
    let mut small_files = Vec::new();
    let mut large_files = Vec::new();

    for entry in walker
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| should_include_file(e.path(), &config.file_extensions, &[]))
    {
        match entry.metadata() {
            Ok(metadata) if metadata.len() < SMALL_FILE_THRESHOLD => small_files.push(entry),
            _ => large_files.push(entry),
        }
    }

    let mut final_result = SearchResult::new();

    // Process small files with simple pattern matching
    if !small_files.is_empty() {
        let results: Vec<_> = if is_simple {
            small_files
                .par_iter()
                .map(|entry| search_file_simple(entry.path(), &config.pattern))
                .filter_map(|r| r.ok())
                .filter(|r| !r.matches.is_empty())
                .collect()
        } else if let Some(ref regex) = regex {
            small_files
                .par_iter()
                .map(|entry| search_file_regex(entry.path(), regex))
                .filter_map(|r| r.ok())
                .filter(|r| !r.matches.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        results
            .into_iter()
            .for_each(|r| final_result.add_file_result(r));
    }

    // Process large files with chunked parallel processing
    if !large_files.is_empty() {
        let chunk_size = (large_files.len() / rayon::current_num_threads())
            .max(MIN_CHUNK_SIZE)
            .min(MAX_CHUNK_SIZE);

        let results: Vec<_> = large_files
            .par_chunks(chunk_size)
            .flat_map(|chunk| {
                chunk
                    .par_iter()
                    .map(|entry| {
                        if let Some(ref regex) = regex {
                            search_file_regex(entry.path(), regex)
                        } else {
                            search_file_simple(entry.path(), &config.pattern)
                        }
                    })
                    .filter_map(|r| r.ok())
                    .filter(|r| !r.matches.is_empty())
                    .collect::<Vec<_>>()
            })
            .collect();

        results
            .into_iter()
            .for_each(|r| final_result.add_file_result(r));
    }

    Ok(final_result)
}
