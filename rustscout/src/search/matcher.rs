use dashmap::DashMap;
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;

use crate::metrics::MemoryMetrics;

const SIMPLE_PATTERN_THRESHOLD: usize = 32;

static PATTERN_CACHE: Lazy<DashMap<String, MatchStrategy>> = Lazy::new(DashMap::new);

/// Strategy for pattern matching
#[derive(Debug, Clone)]
pub enum MatchStrategy {
    Simple(String),
    Regex(Arc<Regex>),
}

/// Handles pattern matching operations
#[derive(Debug, Clone)]
pub struct PatternMatcher {
    strategies: Vec<MatchStrategy>,
    metrics: Arc<MemoryMetrics>,
}

impl PatternMatcher {
    /// Creates a new PatternMatcher for the given patterns
    pub fn new(patterns: Vec<String>) -> Self {
        Self::with_metrics(patterns, Arc::new(MemoryMetrics::new()))
    }

    /// Creates a new PatternMatcher with the specified metrics
    pub fn with_metrics(patterns: Vec<String>, metrics: Arc<MemoryMetrics>) -> Self {
        let mut strategies = Vec::with_capacity(patterns.len());

        for pattern in patterns {
            let strategy = if let Some(entry) = PATTERN_CACHE.get(&pattern) {
                metrics.record_cache_operation(0, true);
                entry.clone()
            } else {
                let strategy = if Self::is_simple_pattern(&pattern) {
                    MatchStrategy::Simple(pattern.clone())
                } else {
                    MatchStrategy::Regex(Arc::new(
                        Regex::new(&pattern).expect("Invalid regex pattern"),
                    ))
                };

                // Record cache miss and size change only once
                metrics.record_cache_operation(pattern.len() as i64, false);

                PATTERN_CACHE.insert(pattern.clone(), strategy.clone());
                strategy
            };
            strategies.push(strategy);
        }

        Self {
            strategies,
            metrics,
        }
    }

    /// Gets the current memory metrics
    pub fn metrics(&self) -> &MemoryMetrics {
        &self.metrics
    }

    /// Determines if a pattern can use simple string matching
    fn is_simple_pattern(pattern: &str) -> bool {
        pattern.len() < SIMPLE_PATTERN_THRESHOLD
            && !pattern.contains(|c: char| c.is_ascii_punctuation() && c != '_' && c != '-')
    }

    /// Finds all matches in the given text
    pub fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        for strategy in &self.strategies {
            match strategy {
                MatchStrategy::Simple(pattern) => {
                    matches.extend(
                        text.match_indices(pattern)
                            .map(|(start, matched)| (start, start + matched.len())),
                    );
                }
                MatchStrategy::Regex(regex) => {
                    matches.extend(regex.find_iter(text).map(|m| (m.start(), m.end())));
                }
            }
        }
        matches.sort_unstable_by_key(|&(start, _)| start);
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pattern_matching() {
        let matcher = PatternMatcher::new(vec!["test".to_string()]);
        let text = "this is a test string with test pattern";
        let matches = matcher.find_matches(text);
        assert_eq!(matches.len(), 2);

        // Verify the exact positions by checking the matched text
        assert_eq!(&text[matches[0].0..matches[0].1], "test");
        assert_eq!(&text[matches[1].0..matches[1].1], "test");
    }

    #[test]
    fn test_regex_pattern_matching() {
        let matcher = PatternMatcher::new(vec![r"\btest\w+".to_string()]);
        let matches = matcher.find_matches("testing tests tested");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_multiple_patterns() {
        let matcher = PatternMatcher::new(vec!["test".to_string(), r"\bword\b".to_string()]);
        let text = "test this word and test another word";
        let matches = matcher.find_matches(text);
        assert_eq!(matches.len(), 4);

        // Verify matches are in order
        let mut prev_start = 0;
        for (start, _) in matches {
            assert!(start >= prev_start);
            prev_start = start;
        }
    }

    #[test]
    fn test_pattern_caching() {
        // Use a unique pattern for this test to avoid interference from other tests
        let unique_pattern = format!("test_pattern_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos());
        
        let metrics = MemoryMetrics::default();
        let metrics = Arc::new(metrics);

        // First creation should have no cache hits and one cache miss
        let _matcher1 = PatternMatcher::with_metrics(vec![unique_pattern.clone()], metrics.clone());
        let first_hits = metrics.cache_hits();
        let first_misses = metrics.cache_misses();
        assert_eq!(first_hits, 0, "First creation should have no cache hits");
        assert_eq!(first_misses, 1, "First creation should have one cache miss");

        // Second creation should hit the cache
        let _matcher2 = PatternMatcher::with_metrics(vec![unique_pattern.clone()], metrics.clone());
        let second_hits = metrics.cache_hits();
        let second_misses = metrics.cache_misses();
        assert_eq!(
            second_hits,
            first_hits + 1,
            "Second creation should add one cache hit"
        );
        assert_eq!(
            second_misses,
            first_misses,
            "Second creation should not add cache misses"
        );

        // Different pattern should not hit the cache
        let different_pattern = format!("{}_different", unique_pattern);
        let _matcher3 = PatternMatcher::with_metrics(vec![different_pattern], metrics.clone());
        let third_hits = metrics.cache_hits();
        let third_misses = metrics.cache_misses();
        assert_eq!(
            third_hits,
            second_hits,
            "Different pattern should not add cache hits"
        );
        assert_eq!(
            third_misses,
            second_misses + 1,
            "Different pattern should add one cache miss"
        );
    }

    #[test]
    fn test_is_simple_pattern() {
        assert!(PatternMatcher::is_simple_pattern("test"));
        assert!(PatternMatcher::is_simple_pattern("hello_world"));
        assert!(!PatternMatcher::is_simple_pattern(r"\btest\w+"));
        assert!(!PatternMatcher::is_simple_pattern("test.*pattern"));
    }
}
