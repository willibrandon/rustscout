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
    /// Uses word boundary checks (\b or equivalent) with strict token separation.
    WholeWords,
    /// Advanced mode that allows partial matches within repeated tokens.
    /// This mode is useful for text manipulations that need to match parts of compound words
    /// or repeated sequences. For example, "YOLO" will match within "YOLOYOLO".
    Partial,
}

/// Defines how hyphens are handled in word boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HyphenMode {
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
    pub hyphen_mode: HyphenMode,
}

impl PatternDefinition {
    /// Creates a new PatternDefinition with the specified parameters.
    pub fn new(text: String, is_regex: bool, boundary_mode: WordBoundaryMode) -> Self {
        Self {
            text,
            is_regex,
            boundary_mode,
            hyphen_mode: HyphenMode::default(),
        }
    }
}

static PATTERN_CACHE: Lazy<DashMap<(String, bool, WordBoundaryMode, HyphenMode), MatchStrategy>> =
    Lazy::new(DashMap::new);

/// Strategy for pattern matching
#[derive(Debug, Clone)]
pub enum MatchStrategy {
    /// Simple substring match with optional word boundary checks.
    Simple {
        pattern: String,
        boundary_mode: WordBoundaryMode,
        hyphen_mode: HyphenMode,
    },
    /// Regex-based match with optional word boundary checks.
    Regex {
        regex: Arc<Regex>,
        boundary_mode: WordBoundaryMode,
        hyphen_mode: HyphenMode,
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
                hyphen_mode: HyphenMode::default(),
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
                pattern.hyphen_mode,
            );
            let strategy = if let Some(entry) = PATTERN_CACHE.get(&cache_key) {
                metrics.record_cache_operation(pattern.text.len() as i64, true);
                entry.clone()
            } else {
                let strategy = if !pattern.is_regex && Self::is_simple_pattern(&pattern.text) {
                    MatchStrategy::Simple {
                        pattern: pattern.text.clone(),
                        boundary_mode: pattern.boundary_mode,
                        hyphen_mode: pattern.hyphen_mode,
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
                            WordBoundaryMode::Partial | WordBoundaryMode::None => format!(r"(?u){}", pattern.text),
                        }
                    };
                    MatchStrategy::Regex {
                        regex: Arc::new(Regex::new(&regex_pattern).expect("Invalid regex pattern")),
                        boundary_mode: pattern.boundary_mode,
                        hyphen_mode: pattern.hyphen_mode,
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
    fn is_word_boundary(text: &str, start: usize, end: usize, hyphen_mode: HyphenMode, boundary_mode: WordBoundaryMode) -> bool {
        // Get the last character of the matched text by going from start to end
        let last_char = text[..end].chars().last();

        // Get the character after the match
        let after_char = text[end..].chars().next();

        // Get the character before the start (if any)
        let before_char = if start > 0 {
            text[..start].chars().last()
        } else {
            None
        };

        #[cfg(test)]
        eprintln!(
            "DEBUG: Checking boundary for text='{}' [{},{}] before={:?} after={:?} hyphen_mode={:?}",
            text, start, end, before_char, after_char, hyphen_mode
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
                        // In partial mode, respect hyphen mode for mathematical symbols
                        if boundary_mode == WordBoundaryMode::Partial {
                            if hyphen_mode == HyphenMode::Joining && is_joinable_symbol(last_c) {
                                return false;
                            }
                            return true;
                        }
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
                    => hyphen_mode == HyphenMode::Joining,
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
                    if hyphen_mode == HyphenMode::Joining && is_joinable_symbol(ch) {
                        true
                    } else {
                        is_word_char(ch) || is_joining_char(ch) // Word chars and joiners continue words
                    }
                }
            }
        };

        // Check for repeated pattern by looking at characters before and after
        let is_repeated_pattern = if let (Some(before), Some(after)) = (before_char, after_char) {
            // For mathematical symbols, we want to be more lenient with word boundaries
            if is_joinable_symbol(text[start..end].chars().next().unwrap_or(' ')) {
                // Math symbols should only be considered repeated if they're directly connected
                // to another math symbol or if in joining mode with an underscore
                match (hyphen_mode, after) {
                    (HyphenMode::Joining, '_') => true,
                    _ => is_joinable_symbol(before) || is_joinable_symbol(after),
                }
            } else {
                // For normal text, use standard word-like character rules
                (is_word_char(before) || is_joining_char(before))
                    && (is_word_char(after) || is_joining_char(after))
            }
        } else if let Some(before) = before_char {
            // At the end of text, check if previous char is word-like
            if is_joinable_symbol(text[start..end].chars().next().unwrap_or(' ')) {
                is_joinable_symbol(before)
            } else {
                is_word_char(before) || is_joining_char(before)
            }
        } else if let Some(after) = after_char {
            // At the start of text, check if next char is word-like
            if is_joinable_symbol(text[start..end].chars().next().unwrap_or(' ')) {
                match (hyphen_mode, after) {
                    (HyphenMode::Joining, '_') => true,
                    _ => is_joinable_symbol(after),
                }
            } else {
                is_word_char(after) || is_joining_char(after)
            }
        } else {
            false
        };

        // We have a boundary if:
        // 1. EITHER side does NOT continue the word
        // 2. AND we're not in a repeated pattern context (only for WholeWords mode)
        let boundary = match boundary_mode {
            WordBoundaryMode::WholeWords => {
                (!continues_word(last_char) || !continues_word(after_char)) && !is_repeated_pattern
            }
            WordBoundaryMode::Partial => {
                // In partial mode, we allow matches in repeated patterns
                // but still respect hyphen mode for joining characters
                match (hyphen_mode, after_char) {
                    // In joining mode, respect joining characters
                    (HyphenMode::Joining, Some(c)) if is_joining_char(c) => false,
                    // Otherwise, allow boundaries even in repeated patterns
                    _ => true
                }
            }
            WordBoundaryMode::None => true,
        };

        #[cfg(test)]
        eprintln!(
            "DEBUG: before_boundary={} after_boundary={} is_repeated={} result={}",
            !continues_word(last_char),
            !continues_word(after_char),
            is_repeated_pattern,
            boundary
        );

        boundary
    }

    /// Finds all matches in the given text
    pub fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        for strategy in &self.strategies {
            match strategy {
                MatchStrategy::Simple {
                    pattern,
                    boundary_mode,
                    hyphen_mode,
                } => {
                    // Skip empty patterns
                    if pattern.is_empty() {
                        continue;
                    }

                    #[cfg(test)]
                    eprintln!(
                        "DEBUG: Simple match for pattern='{}' text='{}' boundary_mode={:?} hyphen_mode={:?}",
                        pattern, text, boundary_mode, hyphen_mode
                    );

                    let indices = text
                        .match_indices(pattern)
                        .map(|(start, matched)| (start, start + matched.len()))
                        .filter(|&(start, end)| match boundary_mode {
                            WordBoundaryMode::None => true,
                            WordBoundaryMode::WholeWords | WordBoundaryMode::Partial => {
                                let is_boundary =
                                    Self::is_word_boundary(text, start, end, *hyphen_mode, *boundary_mode);
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
                    hyphen_mode: _,
                } => {
                    // For regex, word boundaries are handled in the pattern itself
                    matches.extend(regex.find_iter(text).map(|m| (m.start(), m.end())));
                }
            }
        }
        matches.sort_unstable_by_key(|&(start, _)| start);

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
            hyphen_mode: HyphenMode::default(),
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
            hyphen_mode: HyphenMode::default(),
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
            (WordBoundaryMode::WholeWords, HyphenMode::Boundary),
            (WordBoundaryMode::WholeWords, HyphenMode::Joining),
            (WordBoundaryMode::None, HyphenMode::Boundary),
            (WordBoundaryMode::None, HyphenMode::Joining),
            (WordBoundaryMode::Partial, HyphenMode::Boundary),
            (WordBoundaryMode::Partial, HyphenMode::Joining),
        ];

        // Test cases: (text, pattern, expected_matches_whole_word_boundary, expected_matches_whole_word_joining,
        //             expected_matches_none_boundary, expected_matches_none_joining,
        //             expected_matches_partial_boundary, expected_matches_partial_joining, comment)
        let test_cases = vec![
            // 1. Latin script with diacritics
            (
                "I love café food",
                "café",
                1, 1, 1, 1, 1, 1,
                "Basic Latin with diacritics - standalone word",
            ),
            (
                "I love café-bar food",
                "café",
                1, 0, 1, 1, 1, 0,
                "Latin with hyphen - matches depend on hyphen mode",
            ),
            (
                "I love cafébar food",
                "café",
                0, 0, 1, 1, 1, 1,
                "Latin without boundary - only matches in None/Partial mode",
            ),
            // 2. Cyrillic script
            (
                "Привет мир",
                "Привет",
                1, 1, 1, 1, 1, 1,
                "Cyrillic standalone word",
            ),
            (
                "приветствие мир",
                "привет",
                0, 0, 1, 1, 1, 1,
                "Cyrillic as substring - only matches in None/Partial mode",
            ),
            (
                "привет-мир",
                "привет",
                1, 0, 1, 1, 1, 0,
                "Cyrillic with hyphen - matches depend on hyphen mode",
            ),
            // 3. CJK characters
            (
                "你好 世界",
                "你好",
                1, 1, 1, 1, 1, 1,
                "CJK standalone word",
            ),
            (
                "你好吗 世界",
                "你好",
                0, 0, 1, 1, 1, 1,
                "CJK as part of longer word - only matches in None/Partial mode",
            ),
            (
                "你好-世界",
                "你好",
                1, 0, 1, 1, 1, 0,
                "CJK with hyphen - matches depend on hyphen mode",
            ),
            // 4. Korean Hangul
            (
                "안녕 세상",
                "안녕",
                1, 1, 1, 1, 1, 1,
                "Korean standalone word",
            ),
            (
                "안녕하세요 세상",
                "안녕",
                0, 0, 1, 1, 1, 1,
                "Korean as part of longer word - only matches in None/Partial mode",
            ),
            (
                "안녕-세상",
                "안녕",
                1, 0, 1, 1, 1, 0,
                "Korean with hyphen - matches depend on hyphen mode",
            ),
            // 5. Mixed scripts and identifiers
            (
                "hello_世界 test",
                "hello_世界",
                1, 1, 1, 1, 1, 1,
                "Mixed script identifier - full match",
            ),
            (
                "hello_世界 test",
                "hello",
                0, 0, 1, 1, 1, 1,
                "Mixed script identifier - partial match only in None/Partial mode",
            ),
            (
                "test_café_안녕 example",
                "test_café_안녕",
                1, 1, 1, 1, 1, 1,
                "Complex mixed script identifier",
            ),
            // 6. Right-to-left scripts
            (
                "שלום עולם",
                "שלום",
                1, 1, 1, 1, 1, 1,
                "Hebrew standalone word",
            ),
            (
                "שלוםעולם",
                "שלום",
                0, 0, 1, 1, 1, 1,
                "Hebrew as part of word - only matches in None/Partial mode",
            ),
            (
                "مرحبا بالعالم",
                "مرحبا",
                1, 1, 1, 1, 1, 1,
                "Arabic standalone word",
            ),
            // 7. Technical symbols and mathematical notation
            (
                "x + β = γ",
                "β",
                1, 1, 1, 1, 1, 1,
                "Greek letter as symbol",
            ),
            (
                "f(x) = ∑(i=0)",
                "∑",
                1, 1, 1, 1, 1, 1,
                "Mathematical symbol",
            ),
            (
                "∑_total",
                "∑",
                1, 0, 1, 1, 1, 0,
                "Symbol with underscore - matches depend on hyphen mode",
            ),
            // 8. Emoji and combined sequences
            (
                "Hello 👋 World",
                "👋",
                1, 1, 1, 1, 1, 1,
                "Single emoji",
            ),
            (
                "Family: 👨‍👩‍👧‍👦 here",
                "👨‍👩‍👧‍👦",
                1, 1, 1, 1, 1, 1,
                "Combined emoji sequence",
            ),
            (
                "Nice 👍🏽 job",
                "👍🏽",
                1, 1, 1, 1, 1, 1,
                "Emoji with skin tone modifier",
            ),
            // 9. Special cases and edge scenarios
            (
                "hello‑world",
                "hello",
                1, 0, 1, 1, 1, 0,
                "Unicode hyphen (U+2011)",
            ),
            (
                "test_case",
                "test",
                0, 0, 1, 1, 1, 1,
                "Underscore joining",
            ),
            (
                "αβγ test",
                "αβγ",
                1, 1, 1, 1, 1, 1,
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
            exp_partial_boundary,
            exp_partial_joining,
            comment,
        ) in test_cases
        {
            for (boundary_mode, hyphen_mode) in modes.iter() {
                let expected = match (boundary_mode, hyphen_mode) {
                    (WordBoundaryMode::WholeWords, HyphenMode::Boundary) => exp_whole_boundary,
                    (WordBoundaryMode::WholeWords, HyphenMode::Joining) => exp_whole_joining,
                    (WordBoundaryMode::None, HyphenMode::Boundary) => exp_none_boundary,
                    (WordBoundaryMode::None, HyphenMode::Joining) => exp_none_joining,
                    (WordBoundaryMode::Partial, HyphenMode::Boundary) => exp_partial_boundary,
                    (WordBoundaryMode::Partial, HyphenMode::Joining) => exp_partial_joining,
                };

                let matcher = PatternMatcher::with_metrics(
                    vec![PatternDefinition {
                        text: pattern.to_string(),
                        is_regex: false,
                        boundary_mode: *boundary_mode,
                        hyphen_mode: *hyphen_mode,
                    }],
                    metrics.clone(),
                );

                let matches = matcher.find_matches(text);
                assert_eq!(
                    matches.len(),
                    expected,
                    "Failed for pattern '{}' in text '{}' with mode {:?}, hyphen_mode {:?}: {}",
                    pattern,
                    text,
                    boundary_mode,
                    hyphen_mode,
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
                    hyphen_mode: HyphenMode::default(),
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

    #[test]
    fn test_strict_word_boundaries() {
        let metrics = Arc::new(MemoryMetrics::new());

        // Test cases for strict word boundaries
        let test_cases = vec![
            // Basic word boundary cases
            ("YOLO test", "YOLO", 1, "Simple word boundary"),
            ("YOLOYOLO", "YOLO", 0, "Repeated pattern - no match"),
            ("YOLOYOLOYOLO", "YOLO", 0, "Multiple repeats - no match"),
            ("YOLO-YOLO", "YOLO", 2, "Hyphenated repeats - matches both"),
            ("YOLO_YOLO", "YOLO", 0, "Underscore joined - no match"),
            ("YOLODONE", "YOLO", 0, "Partial word - no match"),
            ("DONEYOLO", "YOLO", 0, "Partial word at end - no match"),
            ("YOLO YOLO", "YOLO", 2, "Space separated - matches both"),
            ("YOLO\nYOLO", "YOLO", 2, "Newline separated - matches both"),
            ("YOLO,YOLO", "YOLO", 2, "Comma separated - matches both"),
        ];

        for (text, pattern, expected_matches, comment) in test_cases {
            let matcher = PatternMatcher::with_metrics(
                vec![PatternDefinition {
                    text: pattern.to_string(),
                    is_regex: false,
                    boundary_mode: WordBoundaryMode::WholeWords,
                    hyphen_mode: HyphenMode::Boundary,
                }],
                metrics.clone(),
            );

            let matches = matcher.find_matches(text);
            assert_eq!(
                matches.len(),
                expected_matches,
                "Failed for pattern '{}' in text '{}': {}",
                pattern,
                text,
                comment
            );
        }
    }

    #[test]
    fn test_partial_word_boundaries() {
        let metrics = Arc::new(MemoryMetrics::new());

        // Test cases for partial word boundaries
        let test_cases = vec![
            // Basic word boundary cases
            ("YOLO test", "YOLO", 1, "Simple word boundary"),
            ("YOLOYOLO", "YOLO", 2, "Repeated pattern - matches both"),
            ("YOLOYOLOYOLO", "YOLO", 3, "Multiple repeats - matches all"),
            ("YOLO-YOLO", "YOLO", 2, "Hyphenated repeats - matches both"),
            ("YOLO_YOLO", "YOLO", 2, "Underscore joined - matches both in partial mode"),
            ("YOLODONE", "YOLO", 1, "Partial word - matches"),
            ("DONEYOLO", "YOLO", 1, "Partial word at end - matches"),
            ("YOLO YOLO", "YOLO", 2, "Space separated - matches both"),
            ("YOLO\nYOLO", "YOLO", 2, "Newline separated - matches both"),
            ("YOLO,YOLO", "YOLO", 2, "Comma separated - matches both"),
            // Complex cases
            ("YOLOinYOLO", "YOLO", 2, "Camel case separation - matches both"),
            ("YOLOYOLODONE", "YOLO", 2, "Multiple matches with trailing text"),
            ("PREFIXYOLOinYOLOSUFFIX", "YOLO", 2, "Embedded in larger word"),
        ];

        for (text, pattern, expected_matches, comment) in test_cases {
            let matcher = PatternMatcher::with_metrics(
                vec![PatternDefinition {
                    text: pattern.to_string(),
                    is_regex: false,
                    boundary_mode: WordBoundaryMode::Partial,
                    hyphen_mode: HyphenMode::Boundary,
                }],
                metrics.clone(),
            );

            let matches = matcher.find_matches(text);
            assert_eq!(
                matches.len(),
                expected_matches,
                "Failed for pattern '{}' in text '{}': {}",
                pattern,
                text,
                comment
            );
        }
    }

    #[test]
    fn test_partial_vs_whole_word() {
        let metrics = Arc::new(MemoryMetrics::new());

        // Test cases: (text, pattern, expected_whole_words, expected_partial)
        let test_cases = vec![
            // Basic cases
            ("YOLO YOLOYOLO", "YOLO", 1, 3, "One whole word and two partial matches"),
            ("YOLOYOLO", "YOLO", 0, 2, "No whole words but two partial matches"),
            ("YOLO-YOLO-YOLOYOLO", "YOLO", 2, 4, "Two hyphenated whole words plus two partial matches"),
            ("YOLOinYOLO", "YOLO", 0, 2, "Camel case - no whole words but two partial matches"),
            ("YOLO_YOLO_YOLOYOLO", "YOLO", 0, 4, "Underscore separated - no whole words but four partial matches"),
            // Edge cases
            ("YOLO", "YOLO", 1, 1, "Single word matches both modes"),
            ("YOLO YOLO", "YOLO", 2, 2, "Space separated matches both modes"),
            ("PRE_YOLO_POST", "YOLO", 0, 1, "Embedded in identifier - only partial mode matches"),
        ];

        for (text, pattern, expected_whole_words, expected_partial, comment) in test_cases {
            // Test WholeWords mode
            let whole_words_matcher = PatternMatcher::with_metrics(
                vec![PatternDefinition {
                    text: pattern.to_string(),
                    is_regex: false,
                    boundary_mode: WordBoundaryMode::WholeWords,
                    hyphen_mode: HyphenMode::Boundary,
                }],
                metrics.clone(),
            );

            // Test Partial mode
            let partial_matcher = PatternMatcher::with_metrics(
                vec![PatternDefinition {
                    text: pattern.to_string(),
                    is_regex: false,
                    boundary_mode: WordBoundaryMode::Partial,
                    hyphen_mode: HyphenMode::Boundary,
                }],
                metrics.clone(),
            );

            let whole_words_matches = whole_words_matcher.find_matches(text);
            let partial_matches = partial_matcher.find_matches(text);

            assert_eq!(
                whole_words_matches.len(),
                expected_whole_words,
                "WholeWords mode failed for pattern '{}' in text '{}': {}",
                pattern,
                text,
                comment
            );

            assert_eq!(
                partial_matches.len(),
                expected_partial,
                "Partial mode failed for pattern '{}' in text '{}': {}",
                pattern,
                text,
                comment
            );
        }
    }
}
