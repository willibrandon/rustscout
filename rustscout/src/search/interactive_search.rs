use std::path::PathBuf;
use std::io::{self, Write};
use std::fs;

use colored::Colorize;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
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

/// Mode for the edit session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditMode {
    View,           // Viewing/navigating lines
    LineEdit,       // Editing a specific line
    Replace,        // In replace mode
    SaveConfirm,    // Confirming save
}

/// Actions available during edit mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditAction {
    MovePrev,
    MoveNext,
    StartEdit,
    StartReplace,
    Save,
    Cancel,
    Unknown,
}

/// State for the edit session
struct EditSession {
    file_path: PathBuf,
    lines: Vec<String>,
    current_line: usize,
    mode: EditMode,
    modified: bool,
    match_line: usize,     // The original match line number
    match_start: usize,    // Start offset of match in line
    match_end: usize,      // End offset of match in line
}

impl EditSession {
    fn new(file_path: PathBuf, match_line: usize, match_start: usize, match_end: usize) -> io::Result<Self> {
        let content = fs::read_to_string(&file_path)?;
        let lines: Vec<String> = content.lines().map(String::from).collect();
        
        Ok(Self {
            file_path,
            lines,
            current_line: match_line.saturating_sub(1), // 0-based index
            mode: EditMode::View,
            modified: false,
            match_line: match_line.saturating_sub(1),
            match_start,
            match_end,
        })
    }

    fn save(&self) -> io::Result<()> {
        let content = self.lines.join("\n") + "\n";
        fs::write(&self.file_path, content)
    }

    fn run(&mut self, use_color: bool) -> Result<bool, SearchError> {
        while self.mode != EditMode::SaveConfirm {
            // Clear screen and show content
            print!("{}", Clear(ClearType::All));
            print!("\x1B[H");
            
            // Show header
            let header = format!(
                "=== Edit Mode: {} ===",
                self.file_path.display()
            );
            println!("{}", if use_color { 
                header.bright_blue().bold()
            } else { 
                header.normal()
            });
            println!("Press: [↑/↓] navigate, [Enter] edit line, [r]eplace, [s]ave, [c]ancel\n");

            // Show file content with context
            self.display_content(use_color)?;

            // Handle input based on current mode
            match self.mode {
                EditMode::View => {
                    match self.read_view_action()? {
                        EditAction::MovePrev => {
                            if self.current_line > 0 {
                                self.current_line -= 1;
                            }
                        }
                        EditAction::MoveNext => {
                            if self.current_line < self.lines.len() - 1 {
                                self.current_line += 1;
                            }
                        }
                        EditAction::StartEdit => {
                            self.mode = EditMode::LineEdit;
                        }
                        EditAction::StartReplace => {
                            self.mode = EditMode::Replace;
                        }
                        EditAction::Save => {
                            if self.modified {
                                if let Err(e) = self.save() {
                                    eprintln!("Failed to save: {}", e);
                                    continue;
                                }
                                return Ok(true); // true = file was modified
                            } else {
                                return Ok(false);
                            }
                        }
                        EditAction::Cancel => {
                            return Ok(false);
                        }
                        _ => {}
                    }
                }
                EditMode::LineEdit => {
                    self.edit_current_line(use_color)?;
                    self.mode = EditMode::View;
                }
                EditMode::Replace => {
                    self.do_replace(use_color)?;
                    self.mode = EditMode::View;
                }
                _ => {}
            }
        }
        
        Ok(false)
    }

    fn display_content(&self, use_color: bool) -> Result<(), SearchError> {
        // Show a window of lines around current_line
        let window_size = 5;
        let start = self.current_line.saturating_sub(window_size);
        let end = (self.current_line + window_size + 1).min(self.lines.len());

        for (i, line) in self.lines[start..end].iter().enumerate() {
            let line_num = start + i;
            let line_prefix = if line_num == self.current_line {
                ">".to_string()
            } else {
                " ".to_string()
            };

            let line_display = if line_num == self.match_line {
                // Highlight the matched portion if it still fits within the line
                let mut colored_line = line.clone();
                if use_color && self.match_start < line.len() {
                    let highlight_end = self.match_end.min(line.len());
                    if highlight_end > self.match_start {
                        colored_line.replace_range(
                            self.match_start..highlight_end,
                            &line[self.match_start..highlight_end].bright_green().bold().to_string()
                        );
                    }
                }
                if use_color {
                    colored_line.normal()
                } else {
                    colored_line.normal()
                }
            } else {
                if use_color {
                    line.normal()
                } else {
                    line.normal()
                }
            };

            println!("{}{:>3}: {}", line_prefix, line_num + 1, line_display);
        }
        
        Ok(())
    }

    fn read_view_action(&self) -> Result<EditAction, SearchError> {
        match event::read()
            .map_err(|e| SearchError::config_error(format!("Failed to read event: {}", e)))?
        {
            Event::Key(key) => match key.code {
                KeyCode::Up => Ok(EditAction::MovePrev),
                KeyCode::Down => Ok(EditAction::MoveNext),
                KeyCode::Enter => Ok(EditAction::StartEdit),
                KeyCode::Char('r') | KeyCode::Char('R') => Ok(EditAction::StartReplace),
                KeyCode::Char('s') | KeyCode::Char('S') => Ok(EditAction::Save),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Ok(EditAction::Cancel),
                KeyCode::Char('c') | KeyCode::Char('C') => Ok(EditAction::Cancel),
                KeyCode::Esc => Ok(EditAction::Cancel),
                _ => Ok(EditAction::Unknown),
            },
            _ => Ok(EditAction::Unknown),
        }
    }

    fn edit_current_line(&mut self, _use_color: bool) -> Result<(), SearchError> {
        print!("\r\nEdit line {}: ", self.current_line + 1);
        io::stdout().flush().ok();

        // Read the new line content
        let mut input = String::new();
        io::stdin().read_line(&mut input)
            .map_err(|e| SearchError::config_error(format!("Failed to read line: {}", e)))?;

        let new_content = input.trim();
        if new_content != self.lines[self.current_line] {
            self.lines[self.current_line] = new_content.to_string();
            self.modified = true;
        }

        Ok(())
    }

    fn do_replace(&mut self, _use_color: bool) -> Result<(), SearchError> {
        print!("\r\nSearch pattern: ");
        io::stdout().flush().ok();
        let mut pattern = String::new();
        io::stdin().read_line(&mut pattern)
            .map_err(|e| SearchError::config_error(format!("Failed to read pattern: {}", e)))?;
        let pattern = pattern.trim();

        print!("Replacement text: ");
        io::stdout().flush().ok();
        let mut replacement = String::new();
        io::stdin().read_line(&mut replacement)
            .map_err(|e| SearchError::config_error(format!("Failed to read replacement: {}", e)))?;
        let replacement = replacement.trim();

        print!("Confirm each replacement? (y/N): ");
        io::stdout().flush().ok();
        let mut confirm = String::new();
        io::stdin().read_line(&mut confirm)
            .map_err(|e| SearchError::config_error(format!("Failed to read confirmation: {}", e)))?;
        let mut confirm_replacements = confirm.trim().to_lowercase().starts_with('y');

        let mut modified = false;
        for line in &mut self.lines {
            if confirm_replacements {
                // Show the potential replacement
                if line.contains(pattern) {
                    println!("\nCurrent:  {}", line);
                    let new_line = line.replace(pattern, replacement);
                    println!("Replace with: {}", new_line);
                    print!("Replace? (y/N/a=all): ");
                    io::stdout().flush().ok();

                    let mut response = String::new();
                    io::stdin().read_line(&mut response)
                        .map_err(|e| SearchError::config_error(format!("Failed to read response: {}", e)))?;
                    let response = response.trim().to_lowercase();

                    if response == "a" {
                        // Switch to automatic mode
                        confirm_replacements = false;
                        *line = new_line;
                        modified = true;
                    } else if response.starts_with('y') {
                        *line = new_line;
                        modified = true;
                    }
                }
            } else {
                // Automatic replacement
                if line.contains(pattern) {
                    *line = line.replace(pattern, replacement);
                    modified = true;
                }
            }
        }

        self.modified |= modified;
        Ok(())
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
                let was_modified = open_in_editor(file_path, m.line_number, m.start, m.end, use_color)?;
                enable_raw_mode()?;
                
                if was_modified {
                    // Re-run the search to get updated matches
                    // TODO: Implement re-scanning of the modified file
                    // For now, we'll just continue with the current matches
                    println!("\nFile was modified. Press any key to continue...");
                    let _ = read_key_input()?;
                }
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
        header.bright_blue().bold()
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
fn open_in_editor(file_path: &PathBuf, line: usize, match_start: usize, match_end: usize, use_color: bool) -> Result<bool, SearchError> {
    // Create and run an edit session
    let mut session = EditSession::new(file_path.clone(), line, match_start, match_end)
        .map_err(|e| SearchError::config_error(format!("Failed to create edit session: {}", e)))?;

    // Run the edit session
    session.run(use_color)
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