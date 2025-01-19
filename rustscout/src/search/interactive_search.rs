use std::path::PathBuf;
use colored::Colorize;
use crossterm::{
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};

use crate::{
    SearchError,
    search::search,
    search::matcher::{PatternDefinition, WordBoundaryMode, HyphenMode},
    config::{SearchConfig, EncodingMode},
    cache::ChangeDetectionStrategy,
    results::Match as ScoutMatch,
};

use std::num::NonZeroUsize;

/// Arguments for interactive search
#[derive(Debug)]
pub struct InteractiveSearchArgs {
    pub patterns: Vec<String>,
    pub legacy_patterns: Vec<String>,
    pub is_regex: Vec<bool>,
    pub boundary_mode: String,
    pub word_boundary: bool,
    pub hyphen_mode: String,
    pub root: PathBuf,
    pub extensions: Option<String>,
    pub ignore: Vec<String>,
    pub context_before: usize,
    pub context_after: usize,
    pub threads: Option<NonZeroUsize>,
    pub incremental: bool,
    pub cache_path: Option<PathBuf>,
    pub cache_strategy: String,
    pub encoding: String,
    pub no_color: bool,
}

/// Actions available during interactive search
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    Next,
    Previous,
    SkipFile,
    SkipAll,
    Quit,
    Editor,
    Unknown,
}

/// Statistics for the interactive search session
#[derive(Debug)]
pub struct InteractiveStats {
    pub matches_visited: usize,
    pub matches_skipped: usize,
    pub files_skipped: usize,
    pub total_matches: usize,
}

impl Default for InteractiveStats {
    fn default() -> Self {
        Self {
            matches_visited: 0,
            matches_skipped: 0,
            files_skipped: 0,
            total_matches: 0,
        }
    }
}

/// Flush any pending keyboard/mouse events so we start truly at match #1
fn flush_pending_input() -> Result<(), SearchError> {
    use std::time::Duration;

    // Poll a few times to be safe
    for _ in 0..5 {
        while crossterm::event::poll(Duration::from_millis(10)).unwrap_or(false) {
            let _ = crossterm::event::read(); // discard
        }
    }
    Ok(())
}

/// Run an interactive search session
pub fn run_interactive_search(args: &InteractiveSearchArgs) -> Result<(), SearchError> {
    // Convert args to search config
    let config = convert_args_to_config(args)?;
    
    // Perform the search
    let search_result = search(&config)?;

    // Collect and sort matches
    let mut all_matches: Vec<(PathBuf, ScoutMatch)> = Vec::new();
    for file_res in &search_result.file_results {
        for m in &file_res.matches {
            all_matches.push((file_res.path.clone(), m.clone()));
        }
    }

    // Sort by file path, line number, and match start offset
    all_matches.sort_by(|(path_a, match_a), (path_b, match_b)| {
        let path_cmp = path_a.cmp(path_b);
        if path_cmp != std::cmp::Ordering::Equal {
            path_cmp
        } else {
            let line_cmp = match_a.line_number.cmp(&match_b.line_number);
            if line_cmp != std::cmp::Ordering::Equal {
                line_cmp
            } else {
                // If on same line, sort by start offset
                match_a.start.cmp(&match_b.start)
            }
        }
    });

    if all_matches.is_empty() {
        println!("No matches found.");
        return Ok(());
    }

    println!("Found {} matches in {} files.", 
        search_result.total_matches,
        search_result.files_with_matches
    );

    // Initialize stats and visited flags
    let mut stats = InteractiveStats::default();
    stats.total_matches = all_matches.len();
    let mut visited_flags = vec![false; all_matches.len()];
    let use_color = !args.no_color;

    // Flush any pending input before starting interactive mode
    flush_pending_input()?;

    // Run the interactive loop
    interactive_loop(&all_matches, &mut stats, &mut visited_flags, use_color)?;

    Ok(())
}

fn convert_args_to_config(args: &InteractiveSearchArgs) -> Result<SearchConfig, SearchError> {
    let pattern_defs = args.patterns.iter().map(|p| {
        PatternDefinition {
            text: p.clone(),
            is_regex: args.is_regex.get(0).copied().unwrap_or(false),
            boundary_mode: if args.word_boundary {
                WordBoundaryMode::WholeWords
            } else {
                match args.boundary_mode.as_str() {
                    "strict" => WordBoundaryMode::WholeWords,
                    "partial" => WordBoundaryMode::Partial,
                    _ => WordBoundaryMode::None,
                }
            },
            hyphen_mode: match args.hyphen_mode.as_str() {
                "boundary" => HyphenMode::Boundary,
                _ => HyphenMode::Joining,
            },
        }
    }).collect();

    Ok(SearchConfig {
        pattern_definitions: pattern_defs,
        root_path: args.root.clone(),
        file_extensions: args.extensions.as_ref().map(|e| e.split(',').map(String::from).collect()),
        ignore_patterns: args.ignore.clone(),
        stats_only: false,
        thread_count: args.threads.unwrap_or_else(|| NonZeroUsize::new(4).unwrap()),
        log_level: "info".to_string(),
        context_before: args.context_before,
        context_after: args.context_after,
        incremental: args.incremental,
        cache_path: args.cache_path.clone(),
        cache_strategy: match args.cache_strategy.as_str() {
            "git" => ChangeDetectionStrategy::GitStatus,
            "signature" => ChangeDetectionStrategy::FileSignature,
            _ => ChangeDetectionStrategy::Auto,
        },
        max_cache_size: None,
        use_compression: false,
        encoding_mode: match args.encoding.as_str() {
            "lossy" => EncodingMode::Lossy,
            _ => EncodingMode::FailFast,
        },
    })
}

/// Main interactive loop for processing matches
fn interactive_loop(
    matches: &[(PathBuf, ScoutMatch)], 
    stats: &mut InteractiveStats,
    visited_flags: &mut [bool],
    use_color: bool
) -> Result<(), SearchError> {
    if matches.is_empty() {
        println!("No matches found.");
        return Ok(());
    }

    // Check if we're in test mode
    if std::env::var("INTERACTIVE_TEST").is_ok() {
        // In test mode, just display all matches without interaction
        for (i, (file_path, m)) in matches.iter().enumerate() {
            show_match(i, matches, stats, visited_flags, file_path, m, use_color);
        }
        return Ok(());
    }

    // Regular interactive mode
    enable_raw_mode()?;
    let mut current_index = 0;

    while current_index < matches.len() {
        let (file_path, m) = &matches[current_index];
        
        // Show the current match and update visited status
        show_match(current_index, matches, stats, visited_flags, file_path, m, use_color);
        
        match read_key_input()? {
            PromptAction::Next => {
                // Wrap around to first match if at the end
                if current_index == matches.len() - 1 {
                    current_index = 0;
                } else {
                    current_index += 1;
                }
            }
            PromptAction::Previous => {
                // Wrap around to last match if at the start
                if current_index == 0 {
                    current_index = matches.len() - 1;
                } else {
                    current_index -= 1;
                }
            }
            PromptAction::SkipFile => {
                let current_file = file_path;
                // Mark all unvisited matches in this file as skipped
                let mut skipped = 0;
                for i in 0..matches.len() {
                    if &matches[i].0 == current_file && !visited_flags[i] {
                        visited_flags[i] = true;
                        skipped += 1;
                    }
                }
                stats.matches_skipped += skipped;
                stats.files_skipped += 1;

                // Find next match in a different file
                let mut found_next = false;
                let start_index = current_index;
                for _ in 0..matches.len() {
                    if current_index == matches.len() - 1 {
                        current_index = 0;
                    } else {
                        current_index += 1;
                    }
                    if &matches[current_index].0 != current_file {
                        found_next = true;
                        break;
                    }
                    if current_index == start_index {
                        break;
                    }
                }
                if !found_next {
                    break;
                }
            }
            PromptAction::SkipAll => {
                // Mark all unvisited matches as skipped
                let mut skipped = 0;
                for i in 0..matches.len() {
                    if !visited_flags[i] {
                        visited_flags[i] = true;
                        skipped += 1;
                    }
                }
                stats.matches_skipped += skipped;
                break;
            }
            PromptAction::Quit => break,
            PromptAction::Editor => {
                disable_raw_mode()?;
                open_in_editor(file_path, m.line_number)?;
                enable_raw_mode()?;
            }
            PromptAction::Unknown => {}
        }
    }

    // Cleanup and show summary
    disable_raw_mode()?;
    print_summary(stats);
    Ok(())
}

/// Show a match and update visited status
fn show_match(
    index: usize,
    matches: &[(PathBuf, ScoutMatch)],
    stats: &mut InteractiveStats,
    visited_flags: &mut [bool],
    file_path: &PathBuf,
    m: &ScoutMatch,
    use_color: bool,
) {
    // Update visited status if this is the first time seeing this match
    if !visited_flags[index] {
        visited_flags[index] = true;
        stats.matches_visited += 1;
    }

    // Clear screen and print header
    print!("{}", Clear(ClearType::All));
    print!("\x1B[H");
    
    let header = format!(
        "RustScout Interactive Search :: Match {} of {} ({})",
        index + 1,
        matches.len(),
        file_path.display()
    );
    println!("{}", if use_color { 
        header.bold().bright_blue() 
    } else { 
        header.normal() 
    });
    
    let stats_line = format!(
        "Visited: {}, Skipped: {}, Files skipped: {}",
        stats.matches_visited,
        stats.matches_skipped,
        stats.files_skipped
    );
    println!("{}", if use_color { 
        stats_line.bright_black() 
    } else { 
        stats_line.normal() 
    });
    
    print_context(file_path, m, use_color);
    
    println!("\nNavigation (wrap-around enabled):");
    let nav_help = "[n]ext [p]rev [f]skip file [a]ll skip [q]uit [e]dit";
    println!("{}", if use_color { 
        nav_help.bright_black() 
    } else { 
        nav_help.normal() 
    });
    println!("Arrow keys: ←/→ prev/next, ↑/↓ prev/next");
}

/// Read exactly one KeyEvent from the user and discard any extras
/// to avoid skipping multiple matches at once
fn read_key_input() -> Result<PromptAction, SearchError> {
    // Wait for the first event
    let evt = crossterm::event::read()
        .map_err(|e| SearchError::config_error(format!("Failed to read event: {}", e)))?;

    let action = match evt {
        Event::Key(key) => convert_key_event(&key),
        _ => PromptAction::Unknown,
    };

    // Discard any extra events in the queue
    discard_extra_events()?;

    Ok(action)
}

/// Discard all events in the queue for a short moment
fn discard_extra_events() -> Result<(), SearchError> {
    use std::time::Duration;
    
    let t0 = std::time::Instant::now();
    let max_duration = Duration::from_millis(30);

    while std::time::Instant::now().duration_since(t0) < max_duration {
        if crossterm::event::poll(Duration::from_millis(1))
            .map_err(|e| SearchError::config_error(format!("Failed to poll events: {}", e)))? 
        {
            let _ = crossterm::event::read(); // discard
        } else {
            break;
        }
    }
    Ok(())
}

/// Convert a key event to a PromptAction
fn convert_key_event(event: &KeyEvent) -> PromptAction {
    match event.code {
        KeyCode::Enter | KeyCode::Down | KeyCode::Right => PromptAction::Next,
        KeyCode::Up | KeyCode::Left => PromptAction::Previous,
        KeyCode::Char('n') | KeyCode::Char('N') => PromptAction::Next,
        KeyCode::Char('p') | KeyCode::Char('P') => PromptAction::Previous,
        KeyCode::Char('f') | KeyCode::Char('F') => PromptAction::SkipFile,
        KeyCode::Char('a') | KeyCode::Char('A') => PromptAction::SkipAll,
        KeyCode::Char('q') | KeyCode::Char('Q') => PromptAction::Quit,
        KeyCode::Char('e') | KeyCode::Char('E') => PromptAction::Editor,
        KeyCode::Esc => PromptAction::Quit,
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => PromptAction::Quit,
        _ => PromptAction::Unknown,
    }
}

/// Print the context around a match
fn print_context(file_path: &PathBuf, m: &ScoutMatch, use_color: bool) {
    // Print header with file info
    println!("\n{}", "-".repeat(40));
    let header = format!("File: {}", file_path.display());
    println!("{}", if use_color { 
        header.bright_yellow() 
    } else { 
        header.normal() 
    });

    // Show context before
    for (num, line) in &m.context_before {
        let line_str = format!("   {} | {}", num, line);
        println!("{}", if use_color { 
            line_str.dimmed() 
        } else { 
            line_str.normal() 
        });
    }

    // Highlight the matched line
    let line = if use_color {
        let mut colored_line = m.line_content.clone();
        colored_line.replace_range(m.start..m.end, &m.line_content[m.start..m.end].bright_green().bold().to_string());
        colored_line
    } else {
        m.line_content.clone()
    };
    println!("-> {} | {}", m.line_number, line);

    // Show context after
    for (num, line) in &m.context_after {
        let line_str = format!("   {} | {}", num, line);
        println!("{}", if use_color { 
            line_str.dimmed() 
        } else { 
            line_str.normal() 
        });
    }
}

/// Open the file in an editor at the specified line
fn open_in_editor(file_path: &PathBuf, line: usize) -> Result<(), SearchError> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let status = std::process::Command::new(editor)
        .arg(format!("+{}", line))
        .arg(file_path)
        .status()
        .map_err(|e| SearchError::config_error(format!("Failed to launch editor: {}", e)))?;

    if !status.success() {
        eprintln!("Editor exited with non-zero code.");
    }
    Ok(())
}

/// Print final summary statistics
fn print_summary(stats: &InteractiveStats) {
    println!("\nSearch Summary:");
    println!("  Total matches: {}", stats.total_matches);
    println!("  Matches visited: {}", stats.matches_visited);
    println!("  Matches skipped: {}", stats.matches_skipped);
    println!("  Files skipped: {}", stats.files_skipped);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_prompt_actions() {
        // Navigation keys
        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            PromptAction::Previous
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            PromptAction::Next
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            PromptAction::Previous
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            PromptAction::Next
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            PromptAction::Next
        );

        // Command keys - lowercase
        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            PromptAction::Next
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
            PromptAction::Previous
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE)),
            PromptAction::SkipFile
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            PromptAction::SkipAll
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            PromptAction::Quit
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)),
            PromptAction::Editor
        );

        // Command keys - uppercase
        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE)),
            PromptAction::Next
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('P'), KeyModifiers::NONE)),
            PromptAction::Previous
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('F'), KeyModifiers::NONE)),
            PromptAction::SkipFile
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE)),
            PromptAction::SkipAll
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::NONE)),
            PromptAction::Quit
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('E'), KeyModifiers::NONE)),
            PromptAction::Editor
        );

        // Special keys
        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            PromptAction::Quit
        );

        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            PromptAction::Quit
        );

        // Unknown keys should return Unknown
        assert_eq!(
            convert_key_event(&KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            PromptAction::Unknown
        );
    }
} 