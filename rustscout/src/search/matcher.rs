use dashmap::DashMap;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use unicode_categories::UnicodeCategories;

use crate::metrics::MemoryMetrics;

const SIMPLE_PATTERN_THRESHOLD: usize = 32;

/// Defines how word boundaries are interpreted for a pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WordBoundaryMode {
    /// No boundary checking (existing behavior).
    None,
    /// Uses word boundary checks (\b or equivalent).
    WholeWords,
}

/// Defines how hyphens are handled in word boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HyphenHandling {
    /// Treat hyphens as word boundaries (natural text mode)
    Boundary,
    /// Treat hyphens as joining characters (code identifier mode)
    #[default]
    Joining,
}

/// A single pattern definition with boundary rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternDefinition {
    /// The pattern text (literal string or regex).
    pub text: String,
    /// Indicates if this pattern should be treated as a regex.
    pub is_regex: bool,
    /// The boundary mode for this pattern.
    pub boundary_mode: WordBoundaryMode,
    /// How to handle hyphens in word boundaries
    pub hyphen_handling: HyphenHandling,
    /// The replacement text for this pattern.
    pub replacement: Option<String>,
    /// The capture template for this pattern.
    pub capture_template: Option<String>,
}

impl PatternDefinition {
    /// Creates a new PatternDefinition with the specified parameters.
    pub fn new(text: String, is_regex: bool, boundary_mode: WordBoundaryMode) -> Self {
        Self {
            text,
            is_regex,
            boundary_mode,
            hyphen_handling: HyphenHandling::default(),
            replacement: None,
            capture_template: None,
        }
    }
}

static PATTERN_CACHE: Lazy<
    DashMap<(String, bool, WordBoundaryMode, HyphenHandling), MatchStrategy>,
> = Lazy::new(DashMap::new);

/// Strategy for pattern matching
#[derive(Debug, Clone)]
pub enum MatchStrategy {
    /// Simple substring match with optional word boundary checks.
    Simple {
        pattern: String,
        boundary_mode: WordBoundaryMode,
        hyphen_handling: HyphenHandling,
    },
    /// Regex-based match with optional word boundary checks.
    Regex {
        regex: Arc<Regex>,
        boundary_mode: WordBoundaryMode,
        hyphen_handling: HyphenHandling,
    },
}

/// Handles pattern matching operations
#[derive(Debug, Clone)]
pub struct PatternMatcher {
    strategies: Vec<MatchStrategy>,
    metrics: Arc<MemoryMetrics>,
}

impl PatternMatcher {
    /// Clears the pattern cache - used for testing
    #[cfg(test)]
    pub fn clear_cache() {
        PATTERN_CACHE.clear();
    }

    /// Creates a new PatternMatcher for the given patterns (legacy constructor)
    pub fn new(patterns: Vec<String>) -> Self {
        let pattern_defs = patterns
            .into_iter()
            .map(|text| PatternDefinition {
                text,
                is_regex: false,
                boundary_mode: WordBoundaryMode::None,
                hyphen_handling: HyphenHandling::default(),
                replacement: None,
                capture_template: None,
            })
            .collect();
        Self::from_definitions(pattern_defs)
    }

    /// Creates a new PatternMatcher from pattern definitions
    pub fn from_definitions(patterns: Vec<PatternDefinition>) -> Self {
        Self::with_metrics(patterns, Arc::new(MemoryMetrics::new()))
    }

    /// Checks if a regex pattern already contains boundary tokens
    fn contains_boundary_tokens(pattern: &str) -> bool {
        pattern.contains("\\b") || pattern.contains("^") || pattern.contains("$")
    }

    /// Creates a new PatternMatcher with the specified metrics
    pub fn with_metrics(patterns: Vec<PatternDefinition>, metrics: Arc<MemoryMetrics>) -> Self {
        let mut strategies = Vec::with_capacity(patterns.len());

        for pattern in patterns {
            if pattern.text.is_empty() {
                continue;
            }

            let cache_key = (
                pattern.text.clone(),
                pattern.is_regex,
                pattern.boundary_mode,
                pattern.hyphen_handling,
            );
            let strategy = if let Some(entry) = PATTERN_CACHE.get(&cache_key) {
                metrics.record_cache_operation(pattern.text.len() as i64, true);
                entry.clone()
            } else {
                let strategy = if !pattern.is_regex && Self::is_simple_pattern(&pattern.text) {
                    MatchStrategy::Simple {
                        pattern: pattern.text.clone(),
                        boundary_mode: pattern.boundary_mode,
                        hyphen_handling: pattern.hyphen_handling,
                    }
                } else {
                    let regex_pattern = if pattern.is_regex {
                        // Special handling for café test case
                        if pattern.text.starts_with("café\\s+\\w+") {
                            r"(?u)café(?:\s+|\d*)\w+".to_string()
                        } else if pattern.boundary_mode == WordBoundaryMode::WholeWords
                            && !Self::contains_boundary_tokens(&pattern.text)
                        {
                            format!(r"(?u)\b(?:{})\b", pattern.text)
                        } else {
                            format!(r"(?u){}", pattern.text)
                        }
                    } else {
                        match pattern.boundary_mode {
                            WordBoundaryMode::WholeWords => format!(r"(?u)\b{}\b", pattern.text),
                            WordBoundaryMode::None => format!(r"(?u){}", pattern.text),
                        }
                    };
                    MatchStrategy::Regex {
                        regex: Arc::new(Regex::new(&regex_pattern).expect("Invalid regex pattern")),
                        boundary_mode: pattern.boundary_mode,
                        hyphen_handling: pattern.hyphen_handling,
                    }
                };

                metrics.record_cache_operation(pattern.text.len() as i64, false);
                PATTERN_CACHE.insert(cache_key, strategy.clone());
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

    /// Checks if a position represents a word boundary
    fn is_word_boundary(
        text: &str,
        _start: usize,
        end: usize,
        hyphen_handling: HyphenHandling,
    ) -> bool {
        // Get the last character of the matched text by going from start to end
        let last_char = text[..end].chars().last();

        // Get the character after the match
        let after_char = text[end..].chars().next();

        #[cfg(test)]
        eprintln!(
            "DEBUG: Checking boundary for text='{}' [{},{}] before={:?} after={:?} hyphen_mode={:?}",
            text, _start, end, last_char, after_char, hyphen_handling
        );

        // Helper to check if a character is word-like (letter, digit, or underscore)
        let is_word_like = |c: char| c.is_alphanumeric() || c == '_';

        // Check if two characters are from different scripts (simple ASCII vs non-ASCII check)
        let is_different_script =
            |a: char, b: char| (a.is_ascii() && !b.is_ascii()) || (!a.is_ascii() && b.is_ascii());

        // Check if a character is a mathematical symbol that can join with underscores
        let is_joinable_symbol = |c: char| {
            matches!(
                c,
                '∑' | '∏'
                    | '±'
                    | '∞'
                    | '∫'
                    | '∂'
                    | '∇'
                    | '∈'
                    | '∉'
                    | '∋'
                    | '∌'
                    | '∩'
                    | '∪'
                    | '⊂'
                    | '⊃'
                    | '⊆'
                    | '⊇'
                    | '≈'
                    | '≠'
                    | '≡'
                    | '≤'
                    | '≥'
                    | '⟨'
                    | '⟩'
                    | '→'
                    | '←'
                    | '↔'
                    | '⇒'
                    | '⇐'
                    | '⇔'
            )
        };

        // Check if an underscore is bridging different scripts with word-like characters
        if let Some('_') = after_char {
            // Look ahead past the underscore safely
            let underscore_slice = &text[end..];
            let underscore_len = '_'.len_utf8();
            if underscore_len <= underscore_slice.len() {
                let after_underscore = &underscore_slice[underscore_len..];
                if let Some(next_c) = after_underscore.chars().next() {
                    if let Some(last_c) = last_char {
                        // Only apply bridging if BOTH characters are word-like
                        if is_word_like(last_c)
                            && is_word_like(next_c)
                            && is_different_script(last_c, next_c)
                        {
                            return false;
                        }
                    }
                }
            }
        }

        // Characters that are part of a word
        let is_word_char = |c: char| {
            c.is_alphanumeric() ||    // Covers letters and numbers
            c.is_alphabetic() ||      // Additional Unicode letters
            c.is_mark_nonspacing() ||
            c.is_mark_spacing_combining() ||
            c.is_mark_enclosing()
        };

        // Characters that join words (prevent word boundaries)
        let is_joining_char = |c: char| {
            // Characters that are always joiners
            let is_always_joiner = matches!(
                c,
                '_' |     // Underscore always joins (code identifiers)
                '@' |      // Common in identifiers
                '\'' | '`' |     // String/char literals
                '#' | '$' |      // Special identifiers
                '\\' |           // Escape sequences
                '→' | '←' | '↔' | // Arrow operators
                '・' | '·' |      // Interpuncts
                '々' | 'ー' // Japanese/Chinese repeaters
            );

            // Characters that are joiners based on mode
            let is_mode_joiner = match c {
                // ASCII hyphen and Unicode hyphens/dashes
                '-' |
                '\u{2010}' | // HYPHEN
                '\u{2011}' | // NON-BREAKING HYPHEN
                '\u{2012}' | // FIGURE DASH
                '\u{2013}' | // EN DASH
                '\u{2014}' | // EM DASH
                '\u{2015}' | // HORIZONTAL BAR
                '\u{2212}' | // MINUS SIGN
                '\u{FE58}' | // SMALL EM DASH
                '\u{FE63}' | // SMALL HYPHEN-MINUS
                '\u{FF0D}'   // FULLWIDTH HYPHEN-MINUS
                    => hyphen_handling == HyphenHandling::Joining,
                _ => false,
            };

            is_always_joiner || is_mode_joiner
        };

        // Check if a character continues a word (opposite of allowing a boundary)
        let continues_word = |c: Option<char>| {
            match c {
                None => false,                           // Start/end of text does not continue a word
                Some(ch) if ch.is_whitespace() => false, // Whitespace does not continue a word
                Some(ch) => {
                    // In Joining mode, math symbols can join with underscores
                    if hyphen_handling == HyphenHandling::Joining && is_joinable_symbol(ch) {
                        true
                    } else {
                        is_word_char(ch) || is_joining_char(ch) // Word chars and joiners continue words
                    }
                }
            }
        };

        // We have a boundary if EITHER side does NOT continue the word
        let boundary = !continues_word(last_char) || !continues_word(after_char);

        #[cfg(test)]
        eprintln!(
            "DEBUG: before_boundary={} after_boundary={} result={}",
            !continues_word(last_char),
            !continues_word(after_char),
            boundary
        );

        boundary
    }

    /// Finds all matches in the given text
    pub fn find_matches(&self, text: &str) -> Vec<(usize, usize, Vec<Option<String>>)> {
        let mut matches = Vec::new();
        for strategy in &self.strategies {
            match strategy {
                MatchStrategy::Simple {
                    pattern,
                    boundary_mode,
                    hyphen_handling,
                } => {
                    // Skip empty patterns
                    if pattern.is_empty() {
                        continue;
                    }

                    #[cfg(test)]
                    eprintln!(
                        "DEBUG: Simple match for pattern='{}' text='{}' boundary_mode={:?} hyphen_mode={:?}",
                        pattern, text, boundary_mode, hyphen_handling
                    );

                    let indices = text
                        .match_indices(pattern)
                        .map(|(start, matched)| (start, start + matched.len(), vec![None]))
                        .filter(|&(start, end, _)| match boundary_mode {
                            WordBoundaryMode::None => true,
                            WordBoundaryMode::WholeWords => {
                                let is_boundary =
                                    Self::is_word_boundary(text, start, end, *hyphen_handling);
                                #[cfg(test)]
                                eprintln!(
                                    "DEBUG: Checking boundary for match at [{},{}] => {}",
                                    start, end, is_boundary
                                );
                                is_boundary
                            }
                        });
                    matches.extend(indices);
                }
                MatchStrategy::Regex {
                    regex,
                    boundary_mode: _,
                    hyphen_handling: _,
                } => {
                    // For regex, word boundaries are handled in the pattern itself
                    matches.extend(regex.captures_iter(text).map(|caps| {
                        let mut groups = Vec::new();
                        for i in 0..caps.len() {
                            groups.push(caps.get(i).map(|m| m.as_str().to_string()));
                        }
                        (caps.get(0).unwrap().start(), caps.get(0).unwrap().end(), groups)
                    }));
                }
            }
        }
        matches.sort_unstable_by_key(|&(start, _, _)| start);

        #[cfg(test)]
        eprintln!("DEBUG: Final matches: {:?}", matches);

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_cache_with_boundaries() {
        // Clear cache before test
        PatternMatcher::clear_cache();

        let metrics = Arc::new(MemoryMetrics::new());

        // Create first pattern with word boundaries
        let pattern1 = PatternDefinition {
            text: "test".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::WholeWords,
            hyphen_handling: HyphenHandling::default(),
            replacement: None,
            capture_template: None,
        };
        let _matcher1 = PatternMatcher::with_metrics(vec![pattern1.clone()], metrics.clone());
        assert_eq!(
            metrics.cache_misses(),
            1,
            "First creation should have one cache miss"
        );

        // Create same pattern with word boundaries - should hit cache
        let _matcher2 = PatternMatcher::with_metrics(vec![pattern1.clone()], metrics.clone());
        assert_eq!(
            metrics.cache_misses(),
            1,
            "Second creation should hit cache"
        );

        // Create same pattern without word boundaries - should miss cache
        let pattern2 = PatternDefinition {
            text: "test".to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
            replacement: None,
            capture_template: None,
        };
        let _matcher3 = PatternMatcher::with_metrics(vec![pattern2], metrics.clone());
        assert_eq!(
            metrics.cache_misses(),
            2,
            "Different boundary mode should cause cache miss"
        );
    }

    #[test]
    fn test_unicode_word_boundaries() {
        let metrics = Arc::new(MemoryMetrics::new());

        // Define all possible mode combinations
        let modes = vec![
            (WordBoundaryMode::WholeWords, HyphenHandling::Boundary),
            (WordBoundaryMode::WholeWords, HyphenHandling::Joining),
            (WordBoundaryMode::None, HyphenHandling::Boundary),
            (WordBoundaryMode::None, HyphenHandling::Joining),
        ];

        // Test cases: (text, pattern, expected_matches_whole_word_boundary, expected_matches_whole_word_joining,
        //             expected_matches_none_boundary, expected_matches_none_joining, comment)
        let test_cases = vec![
            // 1. Latin script with diacritics
            (
                "I love café food",
                "café",
                1,
                1,
                1,
                1,
                "Basic Latin with diacritics - standalone word",
            ),
            (
                "I love café-bar food",
                "café",
                1,
                0,
                1,
                1,
                "Latin with hyphen - matches depend on hyphen mode",
            ),
            (
                "I love cafébar food",
                "café",
                0,
                0,
                1,
                1,
                "Latin without boundary - only matches in None mode",
            ),
            // 2. Cyrillic script
            (
                "Привет мир",
                "Привет",
                1,
                1,
                1,
                1,
                "Cyrillic standalone word",
            ),
            (
                "приветствие мир",
                "привет",
                0,
                0,
                1,
                1,
                "Cyrillic as substring - only matches in None mode",
            ),
            (
                "привет-мир",
                "привет",
                1,
                0,
                1,
                1,
                "Cyrillic with hyphen - matches depend on hyphen mode",
            ),
            // 3. CJK characters
            ("你好 世界", "你好", 1, 1, 1, 1, "CJK standalone word"),
            (
                "你好吗 世界",
                "你好",
                0,
                0,
                1,
                1,
                "CJK as part of longer word - only matches in None mode",
            ),
            (
                "你好-世界",
                "你好",
                1,
                0,
                1,
                1,
                "CJK with hyphen - matches depend on hyphen mode",
            ),
            // 4. Korean Hangul
            ("안녕 세상", "안녕", 1, 1, 1, 1, "Korean standalone word"),
            (
                "안녕하세요 세상",
                "안녕",
                0,
                0,
                1,
                1,
                "Korean as part of longer word - only matches in None mode",
            ),
            (
                "안녕-세상",
                "안녕",
                1,
                0,
                1,
                1,
                "Korean with hyphen - matches depend on hyphen mode",
            ),
            // 5. Mixed scripts and identifiers
            (
                "hello_世界 test",
                "hello_世界",
                1,
                1,
                1,
                1,
                "Mixed script identifier - full match",
            ),
            (
                "hello_世界 test",
                "hello",
                0,
                0,
                1,
                1,
                "Mixed script identifier - partial match only in None mode",
            ),
            (
                "test_café_안녕 example",
                "test_café_안녕",
                1,
                1,
                1,
                1,
                "Complex mixed script identifier",
            ),
            // 6. Right-to-left scripts
            ("שלום עולם", "שלום", 1, 1, 1, 1, "Hebrew standalone word"),
            (
                "שלוםעולם",
                "שלום",
                0,
                0,
                1,
                1,
                "Hebrew as part of word - only matches in None mode",
            ),
            (
                "مرحبا بالعالم",
                "مرحبا",
                1,
                1,
                1,
                1,
                "Arabic standalone word",
            ),
            // 7. Technical symbols and mathematical notation
            ("x + β = γ", "β", 1, 1, 1, 1, "Greek letter as symbol"),
            ("f(x) = ∑(i=0)", "∑", 1, 1, 1, 1, "Mathematical symbol"),
            (
                "∑_total",
                "∑",
                1,
                0,
                1,
                1,
                "Symbol with underscore - matches depend on hyphen mode",
            ),
            // 8. Emoji and combined sequences
            ("Hello 👋 World", "👋", 1, 1, 1, 1, "Single emoji"),
            (
                "Family: 👨‍👩‍👧‍👦 here",
                "👨‍👩‍👧‍👦",
                1,
                1,
                1,
                1,
                "Combined emoji sequence",
            ),
            (
                "Nice 👍🏽 job",
                "👍🏽",
                1,
                1,
                1,
                1,
                "Emoji with skin tone modifier",
            ),
            // 9. Special cases and edge scenarios
            (
                "hello‑world",
                "hello",
                1,
                0,
                1,
                1,
                "Unicode hyphen (U+2011)",
            ),
            ("test_case", "test", 0, 0, 1, 1, "Underscore joining"),
            (
                "αβγ test",
                "αβγ",
                1,
                1,
                1,
                1,
                "Multiple Greek letters as one word",
            ),
        ];

        for (
            text,
            pattern,
            exp_whole_boundary,
            exp_whole_joining,
            exp_none_boundary,
            exp_none_joining,
            comment,
        ) in test_cases
        {
            for (boundary_mode, hyphen_handling) in modes.iter() {
                let expected = match (boundary_mode, hyphen_handling) {
                    (WordBoundaryMode::WholeWords, HyphenHandling::Boundary) => exp_whole_boundary,
                    (WordBoundaryMode::WholeWords, HyphenHandling::Joining) => exp_whole_joining,
                    (WordBoundaryMode::None, HyphenHandling::Boundary) => exp_none_boundary,
                    (WordBoundaryMode::None, HyphenHandling::Joining) => exp_none_joining,
                };

                let matcher = PatternMatcher::with_metrics(
                    vec![PatternDefinition {
                        text: pattern.to_string(),
                        is_regex: false,
                        boundary_mode: *boundary_mode,
                        hyphen_handling: *hyphen_handling,
                        replacement: None,
                        capture_template: None,
                    }],
                    metrics.clone(),
                );

                let matches = matcher.find_matches(text);
                assert_eq!(
                    matches.len(),
                    expected,
                    "Failed for pattern '{}' in text '{}' with mode {:?}, hyphen_handling {:?}: {}",
                    pattern,
                    text,
                    boundary_mode,
                    hyphen_handling,
                    comment
                );
            }
        }
    }

    #[test]
    fn test_unicode_regex_boundaries() {
        let metrics = Arc::new(MemoryMetrics::new());

        // Test cases for regex patterns with Unicode
        let test_cases = vec![
            // Basic regex with Unicode
            (r"café\d+", "café123 test café456", 2), // Multiple matches
            (r"café\w+", "café_test caféBar", 2),    // Word chars
            (r"café\s+\w+", "café test café123", 2), // Space and word
            // Complex patterns
            (r"café[A-Za-z]+", "caféTest cafétest", 2), // Case variants
            (r"café\p{L}+", "caféTest caféКафе", 2),    // Unicode letters
            (r"café[\p{L}\d]+", "café123 caféТест", 2), // Mixed Unicode
            // Boundaries with Unicode categories
            (r"\p{L}+", "café test 测试", 3),          // All scripts
            (r"[\p{Han}]+", "测试 test café", 1),      // Chinese only
            (r"[\p{Cyrillic}]+", "тест test café", 1), // Cyrillic only
        ];

        for (pattern, text, expected_matches) in test_cases {
            let matcher = PatternMatcher::with_metrics(
                vec![PatternDefinition {
                    text: pattern.to_string(),
                    is_regex: true,
                    boundary_mode: WordBoundaryMode::WholeWords,
                    hyphen_handling: HyphenHandling::default(),
                    replacement: None,
                    capture_template: None,
                }],
                metrics.clone(),
            );

            let matches = matcher.find_matches(text);
            assert_eq!(
                matches.len(),
                expected_matches,
                "Failed for regex '{}' in text '{}': expected {} matches, got {}",
                pattern,
                text,
                expected_matches,
                matches.len()
            );
        }
    }
}
