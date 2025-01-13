use dashmap::DashMap;
use regex::Regex;
use std::sync::Arc;
use tracing::debug;

// Thresholds for optimization strategies
const SIMPLE_PATTERN_THRESHOLD: usize = 32; // Pattern length for simple search

/// Strategy for pattern matching
#[derive(Debug, Clone)]
pub enum MatchStrategy {
    /// Simple string matching for basic patterns
    Simple(String),
    /// Regex matching for complex patterns
    Regex(Arc<Regex>),
}

/// Global pattern cache for reusing compiled patterns
static PATTERN_CACHE: once_cell::sync::Lazy<DashMap<String, MatchStrategy>> =
    once_cell::sync::Lazy::new(DashMap::new);

/// Handles pattern matching operations
#[derive(Debug, Clone)]
pub struct PatternMatcher {
    strategy: MatchStrategy,
}

impl PatternMatcher {
    /// Creates a new PatternMatcher with the optimal strategy for the given pattern
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        // Try to get from cache first
        if let Some(cached_strategy) = PATTERN_CACHE.get(pattern) {
            debug!("Using cached pattern matcher for: {}", pattern);
            return Ok(Self {
                strategy: cached_strategy.clone(),
            });
        }

        // Create new strategy if not in cache
        let strategy = if Self::is_simple_pattern(pattern) {
            debug!("Using simple string matching for pattern: {}", pattern);
            MatchStrategy::Simple(pattern.to_string())
        } else {
            debug!("Using regex matching for pattern: {}", pattern);
            MatchStrategy::Regex(Arc::new(Regex::new(pattern)?))
        };

        // Cache the strategy
        PATTERN_CACHE.insert(pattern.to_string(), strategy.clone());
        debug!("Cached new pattern matcher for: {}", pattern);

        Ok(Self { strategy })
    }

    /// Determines if a pattern is "simple" enough for optimized literal search
    fn is_simple_pattern(pattern: &str) -> bool {
        let is_simple = pattern.len() < SIMPLE_PATTERN_THRESHOLD
            && !pattern.contains(['*', '+', '?', '[', ']', '(', ')', '|', '^', '$', '.', '\\']);

        debug!(
            "Pattern '{}' is {}",
            pattern,
            if is_simple { "simple" } else { "complex" }
        );
        is_simple
    }

    /// Finds all matches in the given text
    pub fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        match &self.strategy {
            MatchStrategy::Simple(pattern) => text
                .match_indices(pattern)
                .map(|(start, matched)| (start, start + matched.len()))
                .collect(),
            MatchStrategy::Regex(regex) => regex
                .find_iter(text)
                .map(|m| (m.start(), m.end()))
                .collect(),
        }
    }
} 