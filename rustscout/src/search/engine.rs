use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::PathBuf;
use tracing::{debug, info};

use super::matcher::PatternMatcher;
use super::processor::FileProcessor;
use crate::config::SearchConfig;
use crate::errors::SearchResult;
use crate::filters::{should_ignore, should_include_file};
use crate::results::{FileResult, SearchResult as SearchOutput};

/// Performs a concurrent search across files in a directory
pub fn search(config: &SearchConfig) -> SearchResult<SearchOutput> {
    info!("Starting search with patterns: {:?}", config.patterns);

    if config.patterns.is_empty() && config.pattern.is_empty() {
        debug!("No search patterns provided, returning empty result");
        return Ok(SearchOutput::new());
    }

    // Get patterns from either new or legacy field
    let patterns = if !config.patterns.is_empty() {
        config.patterns.clone()
    } else {
        vec![config.pattern.clone()]
    };

    // Create pattern matcher and file processor
    let matcher = PatternMatcher::new(patterns);
    let processor = FileProcessor::new(matcher);
    let metrics = processor.metrics().clone();

    // Set up file walker with ignore patterns
    let mut walker = WalkBuilder::new(&config.root_path);
    walker
        .hidden(true)
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);

    // Add custom ignore patterns
    for pattern in &config.ignore_patterns {
        walker.add_ignore(pattern);
    }

    // Collect files to process
    let files: Vec<PathBuf> = walker
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
        .filter(|entry| {
            let path = entry.path();
            !should_ignore(path, &config.ignore_patterns)
                && should_include_file(path, &config.file_extensions, &config.ignore_patterns)
        })
        .map(|entry| entry.into_path())
        .collect();

    debug!("Found {} files to process", files.len());

    // Process files in parallel with adaptive chunk size
    let thread_count = config.thread_count.get();
    let chunk_size = (files.len() / thread_count).clamp(16, 256);

    let mut result = SearchOutput::new();

    let file_results: Vec<FileResult> = files
        .par_chunks(chunk_size)
        .flat_map(|chunk| {
            chunk
                .iter()
                .filter_map(|path| processor.process_file(path).ok())
                .filter(|result| !result.matches.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();

    // Add results and update statistics
    for file_result in file_results {
        result.add_file_result(file_result);
    }

    // Log memory usage statistics
    metrics.log_stats();

    info!(
        "Search complete. Found {} matches in {} files",
        result.total_matches, result.files_with_matches
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;
    use tempfile::tempdir;

    #[test]
    fn test_search_with_metrics() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "test line\ntest line 2\n").unwrap();

        let config = SearchConfig {
            patterns: vec!["test".to_string()],
            pattern: "test".to_string(),
            root_path: dir.path().to_path_buf(),
            ignore_patterns: vec![],
            file_extensions: None,
            stats_only: false,
            thread_count: NonZeroUsize::new(1).unwrap(),
            log_level: "warn".to_string(),
        };

        let result = search(&config).unwrap();
        assert_eq!(result.files_with_matches, 1);
        assert_eq!(result.total_matches, 2);
    }
}
