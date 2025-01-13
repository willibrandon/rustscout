use ignore::WalkBuilder;
use rayon::prelude::*;
use tracing::{debug, info, warn};

use super::matcher::PatternMatcher;
use super::processor::FileProcessor;
use crate::config::SearchConfig;
use crate::errors::{SearchError, SearchResult};
use crate::filters::should_include_file;
use crate::results::SearchResult as SearchOutput;

// Constants for optimization
const MIN_CHUNK_SIZE: usize = 16; // Minimum files per chunk to reduce overhead
const MAX_CHUNK_SIZE: usize = 256; // Maximum files per chunk to ensure good load balancing

/// Performs a concurrent search across files in the specified directory
pub fn search(config: &SearchConfig) -> SearchResult<SearchOutput> {
    info!("Starting search with pattern: {}", config.pattern);

    if config.pattern.is_empty() {
        warn!("Empty search pattern provided");
        return Ok(SearchOutput::new());
    }

    // Create pattern matcher and file processor
    let matcher = PatternMatcher::new(&config.pattern)
        .map_err(|e| SearchError::invalid_pattern(e.to_string()))?;
    let processor = FileProcessor::new(matcher);

    // Set up the file walker
    let mut builder = WalkBuilder::new(&config.root_path);
    builder
        .hidden(true)
        .standard_filters(true)
        .require_git(false);

    for pattern in &config.ignore_patterns {
        debug!("Adding ignore pattern: {}", pattern);
        builder.add_ignore(pattern);
    }

    builder.add_custom_ignore_filename(".gitignore");
    builder.add_ignore("target");
    builder.add_ignore(".git");

    let walker = builder.build();

    // Collect files to process
    let files: Vec<_> = walker
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| should_include_file(e.path(), &config.file_extensions, &config.ignore_patterns))
        .collect();

    info!("Found {} files to search", files.len());

    // Process files in parallel
    let chunk_size =
        (files.len() / rayon::current_num_threads()).clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE);
    debug!("Using chunk size of {} for parallel processing", chunk_size);

    let results: Vec<_> = files
        .par_chunks(chunk_size)
        .flat_map(|chunk| {
            chunk
                .par_iter()
                .map(|entry| processor.process_file(entry.path()))
                .filter_map(|r| r.ok())
                .filter(|r| !r.matches.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();

    // Combine results
    let mut final_result = SearchOutput::new();
    results
        .into_iter()
        .for_each(|r| final_result.add_file_result(r));

    info!(
        "Search completed: {} matches in {} files",
        final_result.total_matches, final_result.files_with_matches
    );

    Ok(final_result)
}
