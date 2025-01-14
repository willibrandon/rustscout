use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::cache::{create_detector, ChangeStatus, FileSignatureDetector, IncrementalCache};
use crate::config::SearchConfig;
use crate::errors::SearchResult;
use crate::filters::{should_ignore, should_include_file};
use crate::results::{FileResult, SearchResult as SearchOutput};
use crate::search::matcher::PatternMatcher;
use crate::search::processor::FileProcessor;

/// Performs a concurrent search across files in a directory
pub fn search(config: &SearchConfig) -> SearchResult<SearchOutput> {
    info!("Starting search with patterns: {:?}", config.patterns);

    // Return early if all patterns are empty
    if (config.patterns.is_empty() || config.patterns.iter().all(|p| p.is_empty()))
        && config.pattern.is_empty()
    {
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
    let processor = FileProcessor::new(matcher, config.context_before, config.context_after);
    let metrics = processor.metrics().clone();

    // Collect all files to search
    let mut files: Vec<PathBuf> = WalkBuilder::new(&config.root_path)
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
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

    // Sort for consistent ordering
    files.sort();

    let mut result = SearchOutput::new();

    // Handle incremental search if enabled
    if config.incremental {
        debug!("Using incremental search");
        let cache_path = config.get_cache_path();
        let mut cache = IncrementalCache::load_from(&cache_path)?;

        // Detect changed files
        let detector = create_detector(config.cache_strategy, config.root_path.clone());
        let changes = detector.detect_changes(&files)?;

        let mut files_to_search = Vec::new();
        let mut cache_hits = 0;
        let mut total_files = 0;

        for file in files {
            total_files += 1;

            // Check if file has changed
            if let Some(change) = changes.iter().find(|c| c.path == file) {
                match change.status {
                    ChangeStatus::Added | ChangeStatus::Modified => {
                        files_to_search.push(file);
                    }
                    ChangeStatus::Renamed(ref old_path) => {
                        // If we have results for the old path, update the cache
                        if let Some(entry) = cache.files.remove(old_path) {
                            cache.files.insert(file.clone(), entry);
                            cache_hits += 1;
                        } else {
                            files_to_search.push(file);
                        }
                    }
                    ChangeStatus::Deleted => {
                        cache.files.remove(&file);
                    }
                    ChangeStatus::Unchanged => {
                        if let Some(entry) = cache.files.get_mut(&file) {
                            if let Some(matches) = &entry.search_results {
                                let matches = matches.clone();
                                entry.mark_accessed();
                                result.add_file_result(FileResult {
                                    path: file,
                                    matches,
                                });
                                cache_hits += 1;
                            } else {
                                files_to_search.push(file);
                            }
                        } else {
                            files_to_search.push(file);
                        }
                    }
                }
            } else {
                // File not in changes list, treat as unchanged
                if let Some(entry) = cache.files.get_mut(&file) {
                    if let Some(matches) = &entry.search_results {
                        let matches = matches.clone();
                        entry.mark_accessed();
                        result.add_file_result(FileResult {
                            path: file,
                            matches,
                        });
                        cache_hits += 1;
                    } else {
                        files_to_search.push(file);
                    }
                } else {
                    files_to_search.push(file);
                }
            }
        }

        // Update cache statistics
        cache.update_stats(cache_hits, total_files);

        // Process changed files in parallel
        if !files_to_search.is_empty() {
            let chunk_size = (files_to_search.len() / rayon::current_num_threads()).max(1);
            let new_results: Vec<FileResult> = files_to_search
                .par_chunks(chunk_size)
                .flat_map(|chunk| {
                    chunk
                        .iter()
                        .filter_map(|path| processor.process_file(path).ok())
                        .filter(|result| !result.matches.is_empty())
                        .collect::<Vec<_>>()
                })
                .collect();

            // Update cache with new results
            for file_result in &new_results {
                let signature = FileSignatureDetector::compute_signature(&file_result.path)?;
                cache.files.insert(
                    file_result.path.clone(),
                    crate::cache::FileCacheEntry::new(signature),
                );
            }

            // Add new results
            for file_result in new_results {
                result.add_file_result(file_result);
            }
        }

        // Save updated cache
        if let Err(e) = cache.save_to(&cache_path) {
            warn!("Failed to save cache: {}", e);
        }
    } else {
        // Non-incremental search: process all files in parallel
        let chunk_size = (files.len() / rayon::current_num_threads()).max(1);
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

        // Add results
        for file_result in file_results {
            result.add_file_result(file_result);
        }
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
    use crate::ChangeDetectionStrategy;
    use std::num::NonZeroUsize;
    use tempfile::tempdir;

    #[test]
    fn test_search_with_metrics() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

        let config = SearchConfig {
            patterns: vec!["pattern_\\d+".to_string()],
            pattern: "pattern_\\d+".to_string(),
            root_path: file_path.parent().unwrap().to_path_buf(),
            ignore_patterns: vec![],
            file_extensions: None,
            stats_only: false,
            thread_count: NonZeroUsize::new(1).unwrap(),
            log_level: "warn".to_string(),
            context_before: 0,
            context_after: 0,
            incremental: false,
            cache_path: None,
            cache_strategy: ChangeDetectionStrategy::Auto,
            max_cache_size: None,
            use_compression: false,
        };

        let result = search(&config)?;
        assert_eq!(result.files_with_matches, 1);
        assert_eq!(result.total_matches, 2);

        Ok(())
    }

    #[test]
    fn test_incremental_search() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

        let cache_path = dir.path().join("cache.json");
        let config = SearchConfig {
            patterns: vec!["pattern_\\d+".to_string()],
            pattern: "pattern_\\d+".to_string(),
            root_path: file_path.parent().unwrap().to_path_buf(),
            ignore_patterns: vec![],
            file_extensions: None,
            stats_only: false,
            thread_count: NonZeroUsize::new(1).unwrap(),
            log_level: "warn".to_string(),
            context_before: 0,
            context_after: 0,
            incremental: true,
            cache_path: Some(cache_path.clone()),
            cache_strategy: ChangeDetectionStrategy::FileSignature,
            max_cache_size: None,
            use_compression: false,
        };

        // First search should create cache
        let result = search(&config)?;
        assert_eq!(result.files_with_matches, 1);
        assert_eq!(result.total_matches, 2);
        assert!(cache_path.exists());

        // Second search should use cache
        let result = search(&config)?;
        assert_eq!(result.files_with_matches, 1);
        assert_eq!(result.total_matches, 2);

        // Modify file and search again
        std::fs::write(&file_path, "pattern_1\npattern_2\npattern_3\n")?;
        let result = search(&config)?;
        assert_eq!(result.files_with_matches, 1);
        assert_eq!(result.total_matches, 3);

        Ok(())
    }
}
