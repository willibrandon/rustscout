use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::cache::{create_detector, ChangeStatus, FileSignatureDetector, IncrementalCache};
use crate::config::{EncodingMode, SearchConfig};
use crate::errors::{SearchError, SearchResult};
use crate::filters::{should_ignore, should_include_file};
use crate::metrics::MemoryMetrics;
use crate::results::{FileResult, SearchResult as SearchOutput};
use crate::search::matcher::PatternMatcher;
use crate::search::processor::FileProcessor;

/// Performs a concurrent search across files in a directory
pub fn search(config: &SearchConfig) -> SearchResult<SearchOutput> {
    let pattern_defs = config.get_pattern_definitions();
    info!(
        "Starting search with {} pattern definitions",
        pattern_defs.len()
    );

    // Return early if no patterns
    if pattern_defs.is_empty() {
        debug!("No search patterns provided, returning empty result");
        return Ok(SearchOutput::new());
    }

    let metrics = Arc::new(MemoryMetrics::new());
    let matcher = PatternMatcher::with_metrics(pattern_defs, metrics.clone());
    let processor = FileProcessor::new(
        matcher,
        config.context_before,
        config.context_after,
        config.encoding_mode,
    );

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
            !should_ignore(path, &config.root_path, &config.ignore_patterns)
                && should_include_file(
                    path,
                    &config.root_path,
                    &config.file_extensions,
                    &config.ignore_patterns,
                )
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
            let new_results: Result<Vec<FileResult>, _> = files_to_search
                .par_chunks(chunk_size)
                .try_fold(Vec::new, |mut acc, chunk| {
                    for path in chunk {
                        // In FailFast mode, propagate any error
                        if config.encoding_mode == EncodingMode::FailFast {
                            let result = processor.process_file(path)?;
                            if !result.matches.is_empty() {
                                acc.push(result);
                            }
                        } else {
                            // In Lossy mode, skip errors
                            if let Ok(result) = processor.process_file(path) {
                                if !result.matches.is_empty() {
                                    acc.push(result);
                                }
                            }
                        }
                    }
                    Ok::<_, SearchError>(acc)
                })
                .try_reduce(Vec::new, |mut a, mut b| {
                    a.append(&mut b);
                    Ok::<_, SearchError>(a)
                });

            // Handle results based on mode
            let new_results = new_results?;

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
        let file_results: Result<Vec<FileResult>, _> = files
            .par_chunks(chunk_size)
            .try_fold(Vec::new, |mut acc, chunk| {
                for path in chunk {
                    // In FailFast mode, propagate any error
                    if config.encoding_mode == EncodingMode::FailFast {
                        let result = processor.process_file(path)?;
                        if !result.matches.is_empty() {
                            acc.push(result);
                        }
                    } else {
                        // In Lossy mode, skip errors
                        if let Ok(result) = processor.process_file(path) {
                            if !result.matches.is_empty() {
                                acc.push(result);
                            }
                        }
                    }
                }
                Ok::<_, SearchError>(acc)
            })
            .try_reduce(Vec::new, |mut a, mut b| {
                a.append(&mut b);
                Ok::<_, SearchError>(a)
            });

        // Handle results based on mode
        let file_results = file_results?;

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
    use crate::search::matcher::WordBoundaryMode;
    use crate::ChangeDetectionStrategy;
    use tempfile::tempdir;

    #[test]
    fn test_search_with_metrics() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "pattern_1\npattern_2\n")?;

        let mut config = SearchConfig::new_with_pattern(
            "pattern_\\d+".to_string(),
            true,
            WordBoundaryMode::None,
        );
        config.root_path = dir.path().to_path_buf();

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
        let mut config = SearchConfig::new_with_pattern(
            "pattern_\\d+".to_string(),
            true,
            WordBoundaryMode::None,
        );
        config.root_path = file_path.parent().unwrap().to_path_buf();
        config.incremental = true;
        config.cache_path = Some(cache_path.clone());
        config.cache_strategy = ChangeDetectionStrategy::FileSignature;

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

    #[test]
    fn test_word_boundary_search() -> SearchResult<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "test testing tested test-case\n")?;

        // Search with word boundaries
        let mut config =
            SearchConfig::new_with_pattern("test".to_string(), false, WordBoundaryMode::WholeWords);
        config.root_path = file_path.parent().unwrap().to_path_buf();

        let result = search(&config)?;
        assert_eq!(
            result.total_matches, 1,
            "Should only match standalone 'test'"
        );

        // Search without word boundaries
        let mut config =
            SearchConfig::new_with_pattern("test".to_string(), false, WordBoundaryMode::None);
        config.root_path = file_path.parent().unwrap().to_path_buf();

        let result = search(&config)?;
        assert_eq!(
            result.total_matches, 4,
            "Should match all occurrences of 'test'"
        );

        Ok(())
    }
}
