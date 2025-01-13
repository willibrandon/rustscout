use regex::Regex;
use tracing::debug;

// Thresholds for optimization strategies
const SIMPLE_PATTERN_THRESHOLD: usize = 32; // Pattern length for simple search

/// Strategy for pattern matching
#[derive(Debug)]
pub enum MatchStrategy {
    /// Simple string matching for basic patterns
    Simple(String),
    /// Regex matching for complex patterns
    Regex(Regex),
}

/// Handles pattern matching operations
#[derive(Debug)]
pub struct PatternMatcher {
    strategy: MatchStrategy,
}

impl PatternMatcher {
    /// Creates a new PatternMatcher with the optimal strategy for the given pattern
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        let strategy = if Self::is_simple_pattern(pattern) {
            debug!("Using simple string matching for pattern: {}", pattern);
            MatchStrategy::Simple(pattern.to_string())
        } else {
            debug!("Using regex matching for pattern: {}", pattern);
            MatchStrategy::Regex(Regex::new(pattern)?)
        };

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
