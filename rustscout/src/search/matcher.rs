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
    pattern: String,
    strategy: MatchStrategy,
    metrics: Arc<MemoryMetrics>,
}

impl PatternMatcher {
    /// Creates a new PatternMatcher for the given pattern
    pub fn new(pattern: String) -> Self {
        Self::with_metrics(pattern, Arc::new(MemoryMetrics::new()))
    }

    /// Creates a new PatternMatcher with the specified metrics
    pub fn with_metrics(pattern: String, metrics: Arc<MemoryMetrics>) -> Self {
        let strategy = PATTERN_CACHE
            .get(&pattern)
            .map(|entry| {
                metrics.record_cache_operation(0, true);
                entry.clone()
            })
            .unwrap_or_else(|| {
                let strategy = if Self::is_simple_pattern(&pattern) {
                    MatchStrategy::Simple(pattern.clone())
                } else {
                    MatchStrategy::Regex(Arc::new(Regex::new(&pattern).expect("Invalid regex pattern")))
                };
                
                // Record cache miss and size change only once
                metrics.record_cache_operation(pattern.len() as i64, false);
                
                PATTERN_CACHE.insert(pattern.clone(), strategy.clone());
                strategy
            });

        Self {
            pattern,
            strategy,
            metrics,
        }
    }

    /// Gets the current memory metrics
    pub fn metrics(&self) -> &MemoryMetrics {
        &self.metrics
    }

    /// Gets the pattern being used
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Determines if a pattern can use simple string matching
    fn is_simple_pattern(pattern: &str) -> bool {
        pattern.len() < SIMPLE_PATTERN_THRESHOLD
            && !pattern.contains(|c: char| c.is_ascii_punctuation() && c != '_' && c != '-')
    }

    /// Finds all matches in the given text
    pub fn find_matches<'a>(&self, text: &'a str) -> Vec<(usize, usize)> {
        match &self.strategy {
            MatchStrategy::Simple(pattern) => {
                text.match_indices(pattern)
                    .map(|(start, matched)| (start, start + matched.len()))
                    .collect()
            }
            MatchStrategy::Regex(regex) => {
                regex.find_iter(text)
                    .map(|m| (m.start(), m.end()))
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pattern_matching() {
        let matcher = PatternMatcher::new("test".to_string());
        let text = "this is a test string with test pattern";
        let matches = matcher.find_matches(text);
        assert_eq!(matches.len(), 2);
        
        // Verify the exact positions by checking the matched text
        assert_eq!(&text[matches[0].0..matches[0].1], "test");
        assert_eq!(&text[matches[1].0..matches[1].1], "test");
    }

    #[test]
    fn test_regex_pattern_matching() {
        let matcher = PatternMatcher::new(r"\btest\w+".to_string());
        let matches = matcher.find_matches("testing tests tested");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_pattern_caching() {
        // Clear the cache before testing
        PATTERN_CACHE.clear();

        // Create shared metrics
        let metrics = Arc::new(MemoryMetrics::new());

        // First creation should be a cache miss
        let _matcher1 = PatternMatcher::with_metrics("test".to_string(), Arc::clone(&metrics));
        let stats1 = metrics.get_stats();
        assert_eq!(stats1.cache_hits, 0, "First creation should have no cache hits");
        assert_eq!(stats1.cache_misses, 1, "First creation should have one cache miss");

        // Second creation should be a cache hit
        let _matcher2 = PatternMatcher::with_metrics("test".to_string(), Arc::clone(&metrics));
        let stats2 = metrics.get_stats();
        assert_eq!(stats2.cache_hits, 1, "Second creation should have one cache hit");
        assert_eq!(stats2.cache_misses, 1, "Cache misses should not increase on second creation");

        // Third creation should also be a cache hit
        let _matcher3 = PatternMatcher::with_metrics("test".to_string(), Arc::clone(&metrics));
        let stats3 = metrics.get_stats();
        assert_eq!(stats3.cache_hits, 2, "Third creation should have two cache hits");
        assert_eq!(stats3.cache_misses, 1, "Cache misses should still be one");
    }

    #[test]
    fn test_is_simple_pattern() {
        assert!(PatternMatcher::is_simple_pattern("test"));
        assert!(PatternMatcher::is_simple_pattern("hello_world"));
        assert!(!PatternMatcher::is_simple_pattern(r"\btest\w+"));
        assert!(!PatternMatcher::is_simple_pattern("test.*pattern"));
    }
}
