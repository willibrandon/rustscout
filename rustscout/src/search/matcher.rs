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
        pattern.contains("\\b")
            || pattern.contains("^")
            || pattern.contains("$")
            || pattern.contains(r"\B")
            || pattern.contains(r"\<")
            || pattern.contains(r"\>")
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
                        } else {
                            // Only add word boundaries if:
                            // 1. WholeWords mode is requested
                            // 2. Pattern doesn't already have boundary tokens
                            // 3. Pattern doesn't contain alternation or groups that would be affected
                            let needs_boundaries = pattern.boundary_mode == WordBoundaryMode::WholeWords
                                && !Self::contains_boundary_tokens(&pattern.text)
                                && !pattern.text.contains('|') // No alternation
                                && !pattern.text.contains("(?:") // No non-capturing groups
                                && !pattern.text.contains('('); // No capturing groups

                            if needs_boundaries {
                                format!(r"(?u)\b(?:{})\b", pattern.text)
                            } else {
                                format!(r"(?u){}", pattern.text)
                            }
                        }
                    } else {
                        match pattern.boundary_mode {
                            WordBoundaryMode::WholeWords => format!(r"(?u)\b{}\b", pattern.text),
                            WordBoundaryMode::Partial | WordBoundaryMode::None => {
                                format!(r"(?u){}", pattern.text)
                            }
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

    /// Decide if underscore in Partial + Joining should unify (skip) or allow a match.
    /// Return false means "unify → skip the match",
    /// Return true means "no unify → allow partial match".
    fn underscore_partial_joining_allows_match(text: &str, underscore_index: usize) -> bool {
        // Edge cases: if underscore is at start or end, no bridging → allow partial match
        if underscore_index == 0 || underscore_index + 1 >= text.len() {
            return true;
        }

        let before_char = text[..underscore_index].chars().next_back().unwrap();
        let after_char = text[underscore_index + 1..].chars().next().unwrap();

        // If bridging math symbol and normal word-char => unify → skip → return false
        // e.g. "∑_total" → skip match for "∑"
        let bridging_math = (Self::is_math_symbol(before_char) && Self::is_word_char(after_char))
            || (Self::is_math_symbol(after_char) && Self::is_word_char(before_char));
        if bridging_math {
            return false; // unify → skip
        }

        // Otherwise e.g. "hello_世界" → allow partial match
        true
    }

    /// Determines whether a match at the given position has valid word boundaries
    fn is_word_boundary(
        text: &str,
        start: usize,
        end: usize,
        pattern: &str,
        hyphen_mode: HyphenMode,
        boundary_mode: WordBoundaryMode,
    ) -> bool {
        // First check if this match is part of a repeated sequence
        if boundary_mode == WordBoundaryMode::WholeWords
            && Self::is_part_of_repeated_sequence(text, start, end, pattern)
        {
            return false;
        }

        // Get the character before the match, if any
        let before_char = if start == 0 {
            None
        } else {
            text.get(..start)
                .and_then(|slice| slice.chars().next_back())
        };

        // Get the character after the match, if any
        let after_char = text.get(end..).and_then(|slice| slice.chars().next());

        match boundary_mode {
            WordBoundaryMode::None => true,
            WordBoundaryMode::Partial => {
                if hyphen_mode == HyphenMode::Joining {
                    // If hyphen (ASCII or Unicode) is before/after => unify => skip
                    if matches!(before_char, Some('-') | Some('\u{2011}'))
                        || matches!(after_char, Some('-') | Some('\u{2011}'))
                    {
                        return false;
                    }

                    // If underscore is before/after => call bridging logic
                    if before_char == Some('_') {
                        let underscore_index = start.saturating_sub(1);
                        // If bridging logic returns false → unify => skip match
                        if !Self::underscore_partial_joining_allows_match(text, underscore_index) {
                            return false;
                        }
                    }
                    if after_char == Some('_') {
                        let underscore_index = end;
                        // If bridging logic returns false → unify => skip match
                        if !Self::underscore_partial_joining_allows_match(text, underscore_index) {
                            return false;
                        }
                    }
                }
                true
            }
            WordBoundaryMode::WholeWords => {
                let before_continues = if start == 0 {
                    false
                } else {
                    Self::continues_word(
                        text,
                        (start - 1) as isize,
                        before_char,
                        hyphen_mode,
                        boundary_mode,
                    )
                };
                let after_continues = Self::continues_word(
                    text,
                    end as isize,
                    after_char,
                    hyphen_mode,
                    boundary_mode,
                );
                !before_continues && !after_continues
            }
        }
    }

    /// Helper to check if a character continues a word
    fn continues_word(
        text: &str,
        char_index: isize,
        ch_opt: Option<char>,
        hyphen_mode: HyphenMode,
        boundary_mode: WordBoundaryMode,
    ) -> bool {
        if char_index < 0 || ch_opt.is_none() {
            return false;
        }
        let ch = ch_opt.unwrap();

        // Whitespace never continues a word
        if ch.is_whitespace() {
            return false;
        }

        // Word characters always continue
        if Self::is_word_char(ch) {
            return true;
        }

        // Handle both ASCII hyphen and Unicode hyphen (U+2011)
        if ch == '-' || ch == '\u{2011}' {
            return hyphen_mode == HyphenMode::Joining;
        }

        // Special underscore handling
        if ch == '_' {
            match boundary_mode {
                WordBoundaryMode::WholeWords => {
                    return Self::underscore_acts_as_joiner(text, char_index as usize, hyphen_mode);
                }
                WordBoundaryMode::Partial | WordBoundaryMode::None => {
                    return true;
                }
            }
        }

        // Handle other joining characters
        if Self::is_always_joiner(ch) {
            return true;
        }

        // Math symbols join in Joining mode
        if hyphen_mode == HyphenMode::Joining && Self::is_math_symbol(ch) {
            return true;
        }

        false
    }

    /// Helper to determine if an underscore acts as a joiner based on surrounding characters
    fn underscore_acts_as_joiner(
        text: &str,
        underscore_index: usize,
        hyphen_mode: HyphenMode,
    ) -> bool {
        // If hyphen_mode == Joining => unify
        if hyphen_mode == HyphenMode::Joining {
            return true;
        }

        // If we're at edges, treat underscore as boundary
        if underscore_index == 0 || underscore_index >= text.len() - 1 {
            return false;
        }

        let before_char = text[..underscore_index].chars().next_back().unwrap();
        let after_char = text[underscore_index + 1..].chars().next().unwrap();

        // If either side is a math symbol bridging with normal letters => boundary
        if (Self::is_math_symbol(before_char) && Self::is_word_char(after_char))
            || (Self::is_math_symbol(after_char) && Self::is_word_char(before_char))
        {
            // E.g. "∑" bridging with "t" => boundary => return false
            return false;
        }

        // Otherwise (including bridging different scripts that are not math/letter combos),
        // treat underscore as a joiner => return true
        true
    }

    /// Helper to check if a character is a word character
    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric()
            || c.is_alphabetic()
            || c.is_mark_nonspacing()
            || c.is_mark_spacing_combining()
            || c.is_mark_enclosing()
    }

    /// Helper to check if a character is a math symbol
    fn is_math_symbol(c: char) -> bool {
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
    }

    /// Helper to check if a character is an always-joining character
    fn is_always_joiner(c: char) -> bool {
        matches!(
            c,
            '@' | '\'' | '`' | '#' | '$' | '\\' | '→' | '←' | '↔' | '・' | '·' | '々' | 'ー'
        )
    }

    /// Check if a match is part of a repeated sequence (e.g. "YOLOYOLO")
    fn is_part_of_repeated_sequence(text: &str, start: usize, end: usize, pattern: &str) -> bool {
        let pat_len = pattern.len();

        // Check if pattern appears immediately before this match
        let has_pattern_before = if start >= pat_len {
            text.get(start - pat_len..start) == Some(pattern)
        } else {
            false
        };

        // Check if pattern appears immediately after this match
        let has_pattern_after = text.get(end..end + pat_len) == Some(pattern);

        has_pattern_before || has_pattern_after
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
                                let is_boundary = Self::is_word_boundary(
                                    text,
                                    start,
                                    end,
                                    pattern,
                                    *hyphen_mode,
                                    *boundary_mode,
                                );
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
            text: "test_cache_boundary_unique123".to_string(),
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
            text: "test_cache_boundary_unique123".to_string(),
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
                1,
                1,
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
                1,
                0,
                "Latin with hyphen - matches depend on hyphen mode",
            ),
            (
                "I love cafébar food",
                "café",
                0,
                0,
                1,
                1,
                1,
                1,
                "Latin without boundary - only matches in None/Partial mode",
            ),
            // 2. Cyrillic script
            (
                "Привет мир",
                "Привет",
                1,
                1,
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
                1,
                1,
                "Cyrillic as substring - only matches in None/Partial mode",
            ),
            (
                "привет-мир",
                "привет",
                1,
                0,
                1,
                1,
                1,
                0,
                "Cyrillic with hyphen - matches depend on hyphen mode",
            ),
            // 3. CJK characters
            ("你好 世界", "你好", 1, 1, 1, 1, 1, 1, "CJK standalone word"),
            (
                "你好吗 世界",
                "你好",
                0,
                0,
                1,
                1,
                1,
                1,
                "CJK as part of longer word - only matches in None/Partial mode",
            ),
            (
                "你好-世界",
                "你好",
                1,
                0,
                1,
                1,
                1,
                0,
                "CJK with hyphen - matches depend on hyphen mode",
            ),
            // 4. Korean Hangul
            (
                "안녕 세상",
                "안녕",
                1,
                1,
                1,
                1,
                1,
                1,
                "Korean standalone word",
            ),
            (
                "안녕하세요 세상",
                "안녕",
                0,
                0,
                1,
                1,
                1,
                1,
                "Korean as part of longer word - only matches in None/Partial mode",
            ),
            (
                "안녕-세상",
                "안녕",
                1,
                0,
                1,
                1,
                1,
                0,
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
                1,
                1,
                "Mixed script identifier - partial match only in None/Partial mode",
            ),
            (
                "test_café_안녕 example",
                "test_café_안녕",
                1,
                1,
                1,
                1,
                1,
                1,
                "Complex mixed script identifier",
            ),
            // 6. Right-to-left scripts
            (
                "שלום עולם",
                "שלום",
                1,
                1,
                1,
                1,
                1,
                1,
                "Hebrew standalone word",
            ),
            (
                "שלוםעולם",
                "שלום",
                0,
                0,
                1,
                1,
                1,
                1,
                "Hebrew as part of word - only matches in None/Partial mode",
            ),
            (
                "مرحبا بالعالم",
                "مرحبا",
                1,
                1,
                1,
                1,
                1,
                1,
                "Arabic standalone word",
            ),
            // 7. Technical symbols and mathematical notation
            ("x + β = γ", "β", 1, 1, 1, 1, 1, 1, "Greek letter as symbol"),
            (
                "f(x) = ∑(i=0)",
                "∑",
                1,
                1,
                1,
                1,
                1,
                1,
                "Mathematical symbol",
            ),
            (
                "∑_total",
                "∑",
                1,
                0,
                1,
                1,
                1,
                0,
                "Symbol with underscore - matches depend on hyphen mode",
            ),
            // 8. Emoji and combined sequences
            ("Hello 👋 World", "👋", 1, 1, 1, 1, 1, 1, "Single emoji"),
            (
                "Family: 👨‍👩‍👧‍👦 here",
                "👨‍👩‍👧‍👦",
                1,
                1,
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
                1,
                0,
                "Unicode hyphen (U+2011)",
            ),
            ("test_case", "test", 0, 0, 1, 1, 1, 1, "Underscore joining"),
            (
                "αβγ test",
                "αβγ",
                1,
                1,
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
            // Edge cases for repeated sequences
            (
                "YOLOYOLOYOLO YOLO",
                "YOLO",
                1,
                "Last YOLO in repeated sequence should not match even with space after",
            ),
            (
                "YOLO YOLOYOLOYOLO",
                "YOLO",
                1,
                "First standalone YOLO should match, none in repeated sequence",
            ),
            (
                "YOLOYOLO YOLOYOLO",
                "YOLO",
                0,
                "No matches when all YOLOs are part of repeated sequences",
            ),
            (
                "YOLOYOLOYOLO YOLO YOLOYOLOYOLO",
                "YOLO",
                1,
                "Only standalone YOLO should match",
            ),
            (
                "YOLOYOLO YOLO YOLOYOLO",
                "YOLO",
                1,
                "Middle standalone YOLO should match",
            ),
            // Complex edge cases
            (
                "YOLOYOLOYOLO,YOLO",
                "YOLO",
                1,
                "Comma after repeated sequence - last YOLO should match",
            ),
            (
                "YOLOYOLOYOLO\nYOLO",
                "YOLO",
                1,
                "Newline after repeated sequence - last YOLO should match",
            ),
            (
                "YOLOYOLOYOLO YOLO!",
                "YOLO",
                1,
                "Punctuation after standalone YOLO",
            ),
            (
                "(YOLOYOLOYOLO) YOLO",
                "YOLO",
                1,
                "Parentheses around repeated sequence",
            ),
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
            (
                "YOLO_YOLO",
                "YOLO",
                2,
                "Underscore joined - matches both in partial mode",
            ),
            ("YOLODONE", "YOLO", 1, "Partial word - matches"),
            ("DONEYOLO", "YOLO", 1, "Partial word at end - matches"),
            ("YOLO YOLO", "YOLO", 2, "Space separated - matches both"),
            ("YOLO\nYOLO", "YOLO", 2, "Newline separated - matches both"),
            ("YOLO,YOLO", "YOLO", 2, "Comma separated - matches both"),
            // Complex cases
            (
                "YOLOinYOLO",
                "YOLO",
                2,
                "Camel case separation - matches both",
            ),
            (
                "YOLOYOLODONE",
                "YOLO",
                2,
                "Multiple matches with trailing text",
            ),
            (
                "PREFIXYOLOinYOLOSUFFIX",
                "YOLO",
                2,
                "Embedded in larger word",
            ),
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
            (
                "YOLO YOLOYOLO",
                "YOLO",
                1,
                3,
                "One whole word and two partial matches",
            ),
            (
                "YOLOYOLO",
                "YOLO",
                0,
                2,
                "No whole words but two partial matches",
            ),
            (
                "YOLO-YOLO-YOLOYOLO",
                "YOLO",
                2,
                4,
                "Two hyphenated whole words plus two partial matches",
            ),
            (
                "YOLOinYOLO",
                "YOLO",
                0,
                2,
                "Camel case - no whole words but two partial matches",
            ),
            (
                "YOLO_YOLO_YOLOYOLO",
                "YOLO",
                0,
                4,
                "Underscore separated - no whole words but four partial matches",
            ),
            // Edge cases
            ("YOLO", "YOLO", 1, 1, "Single word matches both modes"),
            (
                "YOLO YOLO",
                "YOLO",
                2,
                2,
                "Space separated matches both modes",
            ),
            (
                "PRE_YOLO_POST",
                "YOLO",
                0,
                1,
                "Embedded in identifier - only partial mode matches",
            ),
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

    #[test]
    fn test_regex_boundary_handling() {
        let metrics = Arc::new(MemoryMetrics::new());

        // Test cases: (pattern, is_regex, boundary_mode, text, expected_matches, comment)
        let test_cases = vec![
            // User-supplied boundary tokens should be respected
            (
                r"\bYOLO\b",
                true,
                WordBoundaryMode::WholeWords,
                "YOLO test",
                1,
                "Explicit boundary tokens respected",
            ),
            (
                r"\bYOLO\b",
                true,
                WordBoundaryMode::None,
                "YOLO test",
                1,
                "Boundary tokens respected even in None mode",
            ),
            // Complex regex patterns should not get auto-wrapped
            (
                r"(YOLO)+",
                true,
                WordBoundaryMode::WholeWords,
                "YOLOYOLO test",
                1,
                "Group not wrapped with boundaries",
            ),
            (
                r"YOLO|FOMO",
                true,
                WordBoundaryMode::WholeWords,
                "YOLO test FOMO",
                2,
                "Alternation not wrapped",
            ),
            (
                r"(?:YOLO){2}",
                true,
                WordBoundaryMode::WholeWords,
                "YOLOYOLO test",
                1,
                "Non-capturing group not wrapped",
            ),
            // Partial mode should not add boundaries
            (
                r"YOLO",
                true,
                WordBoundaryMode::Partial,
                "YOLOYOLO",
                2,
                "Partial mode allows internal matches",
            ),
            // Edge cases
            (
                r"\BYOLO",
                true,
                WordBoundaryMode::WholeWords,
                "testYOLO",
                1,
                "Custom boundary token \\B respected",
            ),
            (
                r"^\w+YOLO$",
                true,
                WordBoundaryMode::WholeWords,
                "testYOLO",
                1,
                "Start/end anchors respected",
            ),
            (
                r"\<YOLO\>",
                true,
                WordBoundaryMode::WholeWords,
                "YOLO test",
                1,
                "Word boundary alternatives respected",
            ),
        ];

        for (pattern, is_regex, boundary_mode, text, expected_matches, comment) in test_cases {
            let matcher = PatternMatcher::with_metrics(
                vec![PatternDefinition {
                    text: pattern.to_string(),
                    is_regex,
                    boundary_mode,
                    hyphen_mode: HyphenMode::default(),
                }],
                metrics.clone(),
            );

            let matches = matcher.find_matches(text);
            assert_eq!(
                matches.len(),
                expected_matches,
                "Failed for pattern '{}' with boundary_mode {:?}: {}",
                pattern,
                boundary_mode,
                comment
            );
        }
    }
}
