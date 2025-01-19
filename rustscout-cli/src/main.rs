use std::io::Write;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    errors::SearchError,
    replace::{
        FileReplacementPlan, ReplacementConfig, ReplacementPattern, ReplacementSet,
        ReplacementTask, UndoInfo,
    },
    search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode},
    Match,
};
use tracing_subscriber::{self, EnvFilter};

type Result<T> = std::result::Result<T, SearchError>;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Search for patterns in files
    #[command(visible_alias = "s")]
    Search(Box<CliSearchConfig>),

    /// Replace patterns in files
    #[command(visible_alias = "r")]
    Replace {
        #[command(subcommand)]
        command: ReplaceCommands,
    },

    /// Interactively search through matches one by one
    #[command(visible_alias = "i")]
    InteractiveSearch(Box<InteractiveSearchArgs>),

    /// Manage RustScout workspace configuration
    #[command(visible_alias = "w")]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
}

impl Commands {
    fn verbosity_level(&self) -> &str {
        match self {
            Commands::Search(_) => "info",
            Commands::Replace { .. } => "info",
            Commands::InteractiveSearch(_) => "info",
            Commands::Workspace { .. } => "info",
        }
    }
}

fn setup_logging(level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(|e| SearchError::config_error(format!("Failed to initialize logging: {}", e)))?;

    Ok(())
}

#[derive(Subcommand, Debug)]
enum ReplaceCommands {
    /// Perform a search/replace operation
    Do(ReplaceDo),

    /// Undo a previous replacement operation
    Undo(ReplaceUndo),
}

#[derive(Parser, Debug)]
struct CliSearchConfig {
    /// Pattern to search for (can be specified multiple times)
    #[arg(short = 'p', long = "pattern")]
    patterns: Vec<String>,

    /// Legacy positional patterns (deprecated)
    #[arg(hide = true)]
    legacy_patterns: Vec<String>,

    /// Treat the most recently specified pattern as a regular expression
    #[arg(short = 'r', long = "regex", action = clap::ArgAction::Append)]
    is_regex: Vec<bool>,

    /// Word boundary mode (strict|partial|none)
    #[arg(short = 'b', long = "boundary-mode", default_value = "none")]
    boundary_mode: String,

    /// Match whole words only (shorthand for --boundary-mode strict)
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode")]
    word_boundary: bool,

    /// How to handle hyphens in word boundaries (boundary|joining)
    #[arg(short = 'y', long = "hyphen-mode", default_value = "joining")]
    hyphen_mode: String,

    /// Root directory to search in
    #[arg(short = 'd', long = "root", default_value = ".")]
    root: PathBuf,

    /// File extensions to include (e.g. rs,go,js)
    #[arg(short = 'x', long = "extensions")]
    extensions: Option<String>,

    /// Patterns to ignore (glob format)
    #[arg(short = 'g', long = "ignore")]
    ignore: Vec<String>,

    /// Number of context lines before match
    #[arg(short = 'B', long = "context-before", default_value = "0")]
    context_before: usize,

    /// Number of context lines after match
    #[arg(short = 'A', long = "context-after", default_value = "0")]
    context_after: usize,

    /// Show only statistics, not matches
    #[arg(short = 's', long = "stats")]
    stats: bool,

    /// Number of threads to use
    #[arg(short = 'j', long = "threads")]
    threads: Option<NonZeroUsize>,

    /// Enable incremental search using cache
    #[arg(short = 'I', long = "incremental")]
    incremental: bool,

    /// Path to cache file (default: .rustscout-cache.json)
    #[arg(short = 'C', long = "cache-path")]
    cache_path: Option<PathBuf>,

    /// Strategy for detecting file changes (auto|git|signature)
    #[arg(short = 'S', long = "cache-strategy", default_value = "auto")]
    cache_strategy: String,

    /// Maximum cache size in MB (0 for unlimited)
    #[arg(short = 'M', long = "max-cache-size")]
    max_cache_size: Option<u64>,

    /// Enable cache compression
    #[arg(short = 'Z', long = "compress-cache")]
    compress_cache: bool,

    /// How to handle invalid UTF-8 sequences (failfast|lossy)
    #[arg(short = 'E', long = "encoding", default_value = "failfast")]
    encoding: String,

    /// Disable colored output
    #[arg(short = 'N', long = "no-color")]
    no_color: bool,

    /// Verbosity level for logging (error|warn|info|debug|trace)
    #[arg(short = 'v', long = "verbosity", default_value = "info")]
    log_level: String,
}

#[derive(Parser, Debug)]
struct ReplaceDo {
    /// Pattern to search for
    #[arg(short = 'p', long = "pattern", required = true)]
    pattern: String,

    /// Text to replace matches with
    #[arg(short = 'r', long = "replacement", required = true)]
    replacement: String,

    /// Treat pattern as a regular expression
    #[arg(short = 'x', long = "regex")]
    is_regex: bool,

    /// Word boundary mode (none, partial, strict)
    #[arg(short = 'b', long = "boundary-mode", default_value = "none")]
    boundary_mode: String,

    /// Shorthand for --boundary-mode strict
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode")]
    word_boundary: bool,

    /// How to handle hyphens in boundaries
    #[arg(short = 'y', long = "hyphen-mode", default_value = "joining")]
    hyphen_mode: String,

    /// Configuration file for replacements
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,

    /// Dry run - show what would be changed without making changes
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Number of threads to use
    #[arg(short = 'j', long = "threads")]
    threads: Option<NonZeroUsize>,

    /// Preview diff format (unified|side-by-side)
    #[arg(short = 'd', long = "diff-format", default_value = "unified")]
    diff_format: String,

    /// Paths to process
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

#[derive(Parser, Debug)]
struct ReplaceUndo {
    /// ID of the replacement to undo
    #[arg()]
    id: String,

    /// List all hunks available for partial revert in a given undo operation, but do not revert anything
    #[arg(short = 'l', long = "list-hunks")]
    list_hunks: bool,

    /// A comma-separated list of zero-based hunk indices to revert. If omitted, all hunks are reverted. Example: --hunks 1,3,5
    #[arg(short = 'u', long = "hunks")]
    hunks: Option<String>,

    /// Preview the result of reverting the specified hunks without modifying any files
    #[arg(short = 'p', long = "preview")]
    preview: bool,

    /// Launch an interactive wizard or TUI to select hunks for partial revert
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    /// Skip confirmation prompts; proceed without user input
    #[arg(short = 'f', long = "force", alias = "yes")]
    force: bool,

    /// Directory containing undo information
    #[arg(long = "undo-dir", default_value = ".rustscout/undo")]
    undo_dir: PathBuf,
}

/// Arguments for interactive search
#[derive(Parser, Debug)]
struct InteractiveSearchArgs {
    /// Pattern to search for (can be specified multiple times)
    #[arg(short = 'p', long = "pattern")]
    patterns: Vec<String>,

    /// Legacy positional patterns (deprecated)
    #[arg(hide = true)]
    legacy_patterns: Vec<String>,

    /// Treat the most recently specified pattern as a regular expression
    #[arg(short = 'r', long = "regex", action = clap::ArgAction::Append)]
    is_regex: Vec<bool>,

    /// Word boundary mode (strict|partial|none)
    #[arg(short = 'b', long = "boundary-mode", default_value = "none")]
    boundary_mode: String,

    /// Match whole words only (shorthand for --boundary-mode strict)
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode")]
    word_boundary: bool,

    /// How to handle hyphens in word boundaries (boundary|joining)
    #[arg(short = 'y', long = "hyphen-mode", default_value = "joining")]
    hyphen_mode: String,

    /// Root directory to search in
    #[arg(short = 'd', long = "root", default_value = ".")]
    root: PathBuf,

    /// File extensions to include (e.g. rs,go,js)
    #[arg(short = 'x', long = "extensions")]
    extensions: Option<String>,

    /// Patterns to ignore (glob format)
    #[arg(short = 'g', long = "ignore")]
    ignore: Vec<String>,

    /// Number of context lines before match
    #[arg(short = 'B', long = "context-before", default_value = "2")]
    context_before: usize,

    /// Number of context lines after match
    #[arg(short = 'A', long = "context-after", default_value = "2")]
    context_after: usize,

    /// Number of threads to use
    #[arg(short = 'j', long = "threads")]
    threads: Option<NonZeroUsize>,

    /// Enable incremental search using cache
    #[arg(short = 'I', long = "incremental")]
    incremental: bool,

    /// Path to cache file (default: .rustscout-cache.json)
    #[arg(short = 'C', long = "cache-path")]
    cache_path: Option<PathBuf>,

    /// Strategy for detecting file changes (auto|git|signature)
    #[arg(short = 'S', long = "cache-strategy", default_value = "auto")]
    cache_strategy: String,

    /// How to handle invalid UTF-8 sequences (failfast|lossy)
    #[arg(short = 'E', long = "encoding", default_value = "failfast")]
    encoding: String,

    /// Disable colored output
    #[arg(short = 'N', long = "no-color")]
    no_color: bool,

    /// Verbosity level for logging (error|warn|info|debug|trace)
    #[arg(short = 'v', long = "verbosity", default_value = "info")]
    log_level: String,

    /// Enable verbose output
    #[arg(short = 'V', long = "verbose")]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum WorkspaceCommands {
    /// Initialize a new RustScout workspace
    Init(WorkspaceInit),
}

#[derive(Parser, Debug)]
struct WorkspaceInit {
    /// Directory to initialize workspace in (defaults to current directory)
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,

    /// Format to use for workspace metadata (json or yaml)
    #[arg(short = 'f', long = "format", default_value = "json")]
    format: String,

    /// Force initialization even if .rustscout directory exists
    #[arg(short = 'F', long = "force")]
    force: bool,
}

mod diff_utils;
use diff_utils::{print_side_by_side_diff, print_unified_diff};

/// Runs an interactive wizard in the terminal to pick hunks. Returns the set of chosen hunk indices.
fn interactive_select_hunks(info: &UndoInfo) -> Result<Vec<usize>> {
    let mut global_idx = 0;
    let mut mapping = Vec::new(); // (global_idx, file_idx, hunk_idx)
    let mut choices = Vec::new();

    // First pass: show hunks and build mapping
    println!("\nOperation {} ({})", info.timestamp, info.description);
    for (f_idx, file_diff) in info.file_diffs.iter().enumerate() {
        println!("\nFile: {}", file_diff.file_path.display());
        for (h_idx, hunk) in file_diff.hunks.iter().enumerate() {
            mapping.push((global_idx, f_idx, h_idx));

            // Show hunk header with clearer line range format
            let range_text = if hunk.original_line_count == 1 {
                format!("line {}", hunk.original_start_line)
            } else {
                format!(
                    "lines {}–{}",
                    hunk.original_start_line,
                    hunk.original_start_line + hunk.original_line_count - 1
                )
            };

            println!(
                "  [ ] Hunk {} (Global index {}): {}",
                h_idx, global_idx, range_text
            );

            // Show hunk content with line numbers
            println!("    Original:");
            for (i, line) in hunk.original_lines.iter().enumerate() {
                let line_num = hunk.original_start_line + i;
                println!("      {: >4}: {}", line_num, line);
            }
            println!("    Current:");
            for (i, line) in hunk.new_lines.iter().enumerate() {
                let line_num = hunk.new_start_line + i;
                println!("      {: >4}: {}", line_num, line);
            }
            global_idx += 1;
        }
    }

    println!("\nEnter hunk indexes to revert (comma-separated), or press Enter to revert all. Type 'q' to cancel.\n> ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.eq_ignore_ascii_case("q") {
        return Err(SearchError::config_error("User canceled"));
    }

    if input.is_empty() {
        // Revert all hunks
        choices.extend(0..global_idx);
    } else {
        // Parse user input
        for part in input.split(',') {
            match part.trim().parse::<usize>() {
                Ok(idx) if idx < global_idx => {
                    choices.push(idx);
                }
                _ => {
                    println!("Warning: invalid hunk index '{}' ignored", part.trim());
                }
            }
        }
    }

    // Sort and deduplicate
    choices.sort_unstable();
    choices.dedup();

    // Preview selected hunks
    if !choices.is_empty() {
        println!("\nSelected hunks to revert:");
        for &idx in &choices {
            if let Some(&(_, f_idx, h_idx)) = mapping.iter().find(|&&(g, _, _)| g == idx) {
                let file_diff = &info.file_diffs[f_idx];
                let hunk = &file_diff.hunks[h_idx];
                let range_text = if hunk.original_line_count == 1 {
                    format!("line {}", hunk.original_start_line)
                } else {
                    format!(
                        "lines {}–{}",
                        hunk.original_start_line,
                        hunk.original_start_line + hunk.original_line_count - 1
                    )
                };
                println!(
                    "  File: {}, Hunk {} ({})",
                    file_diff.file_path.display(),
                    h_idx,
                    range_text
                );
            }
        }

        // Final confirmation
        print!("\nProceed with reverting these hunks? [y/N] ");
        std::io::stdout().flush()?;
        let mut confirm = String::new();
        std::io::stdin().read_line(&mut confirm)?;
        if !confirm.trim().eq_ignore_ascii_case("y") {
            return Err(SearchError::config_error("User canceled"));
        }
    }

    Ok(choices)
}

fn main() -> Result<()> {
    run()
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    setup_logging(cli.command.verbosity_level())?;

    match cli.command {
        Commands::Search(args) => {
            handle_search(*args)?;
        }
        Commands::Replace { command } => {
            handle_replace(command)?;
        }
        Commands::InteractiveSearch(args) => {
            handle_interactive_search(*args)?;
        }
        Commands::Workspace { command } => {
            handle_workspace(command)?;
        }
    }
    Ok(())
}

fn handle_interactive_search(args: InteractiveSearchArgs) -> Result<()> {
    let lib_args = rustscout::search::interactive_search::InteractiveSearchArgs {
        patterns: args.patterns,
        legacy_patterns: args.legacy_patterns,
        is_regex: args.is_regex,
        boundary_mode: args.boundary_mode,
        word_boundary: args.word_boundary,
        hyphen_mode: args.hyphen_mode,
        root: args.root,
        extensions: args.extensions,
        ignore: args.ignore,
        context_before: args.context_before,
        context_after: args.context_after,
        threads: args.threads,
        incremental: args.incremental,
        cache_path: args.cache_path,
        cache_strategy: args.cache_strategy,
        encoding: args.encoding,
        no_color: args.no_color,
        verbose: args.verbose,
    };

    rustscout::search::interactive_search::run_interactive_search(&lib_args)?;
    Ok(())
}

fn handle_search(args: CliSearchConfig) -> Result<()> {
    let mut pattern_defs = Vec::new();

    // Convert CLI patterns to pattern definitions
    for (i, pattern) in args
        .patterns
        .iter()
        .chain(args.legacy_patterns.iter())
        .enumerate()
    {
        let boundary_mode = if args.word_boundary {
            WordBoundaryMode::WholeWords
        } else {
            match args.boundary_mode.as_str() {
                "strict" => WordBoundaryMode::WholeWords,
                "partial" => WordBoundaryMode::Partial,
                "none" => WordBoundaryMode::None,
                _ => {
                    return Err(SearchError::config_error(format!(
                        "Invalid boundary mode '{}'. Valid values are: strict, partial, none",
                        args.boundary_mode
                    )))
                }
            }
        };

        pattern_defs.push(PatternDefinition {
            text: pattern.clone(),
            is_regex: i < args.is_regex.len() && args.is_regex[i],
            boundary_mode,
            hyphen_mode: match args.hyphen_mode.as_str() {
                "boundary" => HyphenMode::Boundary,
                "joining" => HyphenMode::Joining,
                _ => {
                    return Err(SearchError::config_error(
                        "Invalid hyphen mode. Valid values are: boundary, joining",
                    ))
                }
            },
        });
    }

    let file_extensions = args.extensions.as_ref().map(|e| {
        e.split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    });

    let cache_strategy = match args.cache_strategy.as_str() {
        "git" => ChangeDetectionStrategy::GitStatus,
        "signature" => ChangeDetectionStrategy::FileSignature,
        _ => ChangeDetectionStrategy::Auto,
    };

    let encoding_mode = match args.encoding.to_lowercase().as_str() {
        "lossy" => EncodingMode::Lossy,
        _ => EncodingMode::FailFast,
    };

    let search_config = SearchConfig {
        pattern_definitions: pattern_defs,
        root_path: args.root,
        file_extensions,
        ignore_patterns: args.ignore,
        stats_only: args.stats,
        thread_count: args
            .threads
            .unwrap_or_else(|| NonZeroUsize::new(4).unwrap()),
        log_level: "info".to_string(),
        context_before: args.context_before,
        context_after: args.context_after,
        incremental: args.incremental,
        cache_path: args.cache_path,
        cache_strategy,
        max_cache_size: args.max_cache_size.map(|size| size * 1024 * 1024),
        use_compression: args.compress_cache,
        encoding_mode,
    };

    let result = rustscout::search::search(&search_config)?;

    if args.stats {
        println!(
            "{} matches across {} files",
            result.total_matches, result.files_with_matches
        );
        return Ok(());
    }

    // Print matches in ripgrep style
    for file_result in &result.file_results {
        let file_content = std::fs::read_to_string(&file_result.path)?;
        let all_lines: Vec<&str> = file_content.lines().collect();

        // Track which lines we've printed to avoid duplicates when showing context
        let mut printed_lines = std::collections::HashSet::new();

        // Group matches by their line number
        let mut line_to_matches: std::collections::HashMap<usize, Vec<&Match>> =
            std::collections::HashMap::new();
        for m in &file_result.matches {
            line_to_matches.entry(m.line_number).or_default().push(m);
        }

        // Get sorted line numbers
        let mut line_numbers: Vec<_> = line_to_matches.keys().copied().collect();
        line_numbers.sort();

        // Process lines in order
        for line_num in line_numbers {
            if line_num == 0 || line_num > all_lines.len() {
                continue;
            }

            // Print context before if not already printed
            for ctx_line_num in (line_num.saturating_sub(args.context_before))..line_num {
                if ctx_line_num > 0 && printed_lines.insert(ctx_line_num) {
                    println!(
                        "{}:{}-{}",
                        file_result.path.display(),
                        ctx_line_num,
                        all_lines[ctx_line_num - 1]
                    );
                }
            }

            // Print the matching line with all matches highlighted
            if printed_lines.insert(line_num) {
                let line = all_lines[line_num - 1];
                let matches_in_line = &line_to_matches[&line_num];

                // Sort matches by their start position
                let mut sorted = matches_in_line.clone();
                sorted.sort_by_key(|m| m.start);

                let mut highlighted_line = String::new();
                let mut last_offset = 0;

                for m in sorted {
                    // Add non-highlighted prefix
                    highlighted_line.push_str(&line[last_offset..m.start]);

                    // Add the highlighted match
                    if args.no_color {
                        highlighted_line.push_str(&line[m.start..m.end]);
                    } else {
                        highlighted_line
                            .push_str(&format!("\x1b[1;31m{}\x1b[0m", &line[m.start..m.end]));
                    }

                    last_offset = m.end;
                }

                // Add any remaining non-highlighted suffix
                highlighted_line.push_str(&line[last_offset..]);

                println!(
                    "{}:{}:{}",
                    file_result.path.display(),
                    line_num,
                    highlighted_line
                );
            }

            // Print context after if not already printed
            let end_ctx = (line_num + args.context_after).min(all_lines.len());
            for ctx_line_num in (line_num + 1)..=end_ctx {
                if printed_lines.insert(ctx_line_num) {
                    println!(
                        "{}:{}-{}",
                        file_result.path.display(),
                        ctx_line_num,
                        all_lines[ctx_line_num - 1]
                    );
                }
            }
        }
    }

    println!(
        "\n{} matches across {} files",
        result.total_matches, result.files_with_matches
    );
    Ok(())
}

fn handle_replace(command: ReplaceCommands) -> Result<()> {
    match command {
        ReplaceCommands::Do(do_command) => {
            // Load config file if provided
            let mut repl_config = if let Some(config_path) = do_command.config {
                ReplacementConfig::load_from(&config_path)?
            } else {
                ReplacementConfig {
                    patterns: vec![],
                    backup_enabled: true,
                    dry_run: do_command.dry_run,
                    backup_dir: None,
                    preserve_metadata: true,
                    undo_dir: PathBuf::from(".rustscout").join("undo"),
                }
            };

            let target_paths = if do_command.paths.is_empty() {
                vec![PathBuf::from(".")] // Default to current directory if no paths provided
            } else {
                do_command.paths
            };

            // Create pattern definition
            let boundary_mode = if do_command.word_boundary {
                WordBoundaryMode::WholeWords
            } else {
                match do_command.boundary_mode.as_str() {
                    "strict" => WordBoundaryMode::WholeWords,
                    "partial" => WordBoundaryMode::Partial,
                    "none" => WordBoundaryMode::None,
                    _ => {
                        return Err(SearchError::config_error(format!(
                            "Invalid boundary mode '{}'. Valid values are: strict, partial, none",
                            do_command.boundary_mode
                        )))
                    }
                }
            };

            let pattern_def = PatternDefinition {
                text: do_command.pattern.clone(),
                is_regex: do_command.is_regex,
                boundary_mode,
                hyphen_mode: match do_command.hyphen_mode.as_str() {
                    "boundary" => HyphenMode::Boundary,
                    "joining" => HyphenMode::Joining,
                    _ => {
                        return Err(SearchError::config_error(
                            "Invalid hyphen mode. Valid values are: boundary, joining",
                        ))
                    }
                },
            };

            let replacement_pattern = ReplacementPattern {
                definition: pattern_def.clone(),
                replacement_text: do_command.replacement.clone(),
            };

            // Add pattern to config
            repl_config.patterns.push(replacement_pattern);

            // Create replacement set
            let mut replacement_set = ReplacementSet::new(repl_config.clone());

            // First, find all matches using the search functionality
            let search_config = SearchConfig {
                pattern_definitions: vec![pattern_def],
                root_path: PathBuf::from("."),
                file_extensions: None,
                ignore_patterns: vec![],
                stats_only: false,
                thread_count: do_command
                    .threads
                    .unwrap_or_else(|| NonZeroUsize::new(4).unwrap()),
                log_level: "info".to_string(),
                context_before: 0,
                context_after: 0,
                incremental: false,
                cache_path: None,
                cache_strategy: ChangeDetectionStrategy::FileSignature,
                max_cache_size: None,
                use_compression: false,
                encoding_mode: EncodingMode::FailFast,
            };

            // Process each target path
            for path in target_paths {
                if path.is_file() {
                    let mut plan = FileReplacementPlan::new(path.clone())?;
                    // Search for matches in this file
                    let search_result = rustscout::search::search(&SearchConfig {
                        root_path: path.clone(),
                        ..search_config.clone()
                    })?;

                    // Create a replacement task for each match
                    if let Some(file_result) = search_result.file_results.first() {
                        let content = std::fs::read_to_string(&path)?;

                        // Create a map of line number to byte offset
                        let mut line_offsets = vec![0];
                        for (i, c) in content.char_indices() {
                            if c == '\n' {
                                line_offsets.push(i + 1);
                            }
                        }
                        line_offsets.push(content.len());

                        for m in &file_result.matches {
                            // Convert line-relative positions to absolute file positions
                            let line_offset = line_offsets[m.line_number - 1];
                            let abs_start = line_offset + m.start;
                            let abs_end = line_offset + m.end;

                            let task = ReplacementTask::new(
                                path.clone(),
                                (abs_start, abs_end),
                                do_command.replacement.clone(),
                                0,
                                repl_config.clone(),
                            );
                            plan.add_replacement(task)?;
                        }
                        replacement_set.add_plan(plan);
                    }
                } else if path.is_dir() {
                    // Search for matches in all files in the directory
                    let search_result = rustscout::search::search(&SearchConfig {
                        root_path: path.clone(),
                        ..search_config.clone()
                    })?;

                    // Create plans for each file with matches
                    for file_result in &search_result.file_results {
                        let mut plan = FileReplacementPlan::new(file_result.path.clone())?;
                        for m in &file_result.matches {
                            let task = ReplacementTask::new(
                                file_result.path.clone(),
                                (m.start, m.end),
                                do_command.replacement.clone(),
                                0,
                                repl_config.clone(),
                            );
                            plan.add_replacement(task)?;
                        }
                        replacement_set.add_plan(plan);
                    }
                }
            }

            // Execute replacements
            if do_command.dry_run {
                println!("Dry run - no changes will be made");
            }

            // Always show the preview
            for plan in &replacement_set.plans {
                let (old_content, new_content) = plan.preview_old_new()?;
                match do_command.diff_format.as_str() {
                    "unified" => print_unified_diff(&plan.file_path, &old_content, &new_content),
                    "side-by-side" => {
                        print_side_by_side_diff(&plan.file_path, &old_content, &new_content)
                    }
                    _ => print_unified_diff(&plan.file_path, &old_content, &new_content),
                }
            }

            // Apply changes if not a dry run
            if !do_command.dry_run {
                let _backups = replacement_set.apply_with_progress()?;
                println!("Replacements applied successfully.");
            }

            Ok(())
        }
        ReplaceCommands::Undo(undo_command) => handle_undo(&undo_command),
    }
}

fn handle_undo(undo_command: &ReplaceUndo) -> Result<()> {
    // Check for conflicting flags
    if undo_command.interactive && undo_command.hunks.is_some() {
        return Err(SearchError::config_error(
            "Cannot use --interactive and --hunks together. Please use one or the other.",
        ));
    }

    let config = ReplacementConfig {
        undo_dir: undo_command.undo_dir.clone(),
        ..Default::default()
    };

    let id = undo_command
        .id
        .parse::<u64>()
        .map_err(|e| SearchError::config_error(format!("Invalid undo ID: {}", e)))?;

    // Load the undo info first to check if it exists and has diffs
    let info_path = config.undo_dir.join(format!("{}.json", id));
    let content = std::fs::read_to_string(&info_path)
        .map_err(|e| SearchError::config_error(format!("Failed to read undo info: {}", e)))?;
    let info: UndoInfo = serde_json::from_str(&content)
        .map_err(|e| SearchError::config_error(format!("Failed to parse undo info: {}", e)))?;

    // If there are no diffs, we can only do a full revert
    if info.file_diffs.is_empty() {
        if undo_command.hunks.is_some() || undo_command.list_hunks || undo_command.interactive {
            return Err(SearchError::config_error(
                "This undo operation only supports full-file backups; partial revert is not possible.",
            ));
        }
        if undo_command.preview {
            println!("Preview of full file revert for operation {}:", id);
            for (original, backup) in &info.backups {
                let backup_path = backup.get_abs_path()?;
                let original_path = original.get_abs_path()?;
                let backup_content = std::fs::read_to_string(&backup_path)?;
                let current_content = std::fs::read_to_string(&original_path)?;
                print_unified_diff(&original_path, &current_content, &backup_content);
            }
            return Ok(());
        }
        ReplacementSet::undo_by_id(id, &config)?;
        println!("Successfully restored files from backup {}", id);
        return Ok(());
    }

    // Handle --list-hunks
    if undo_command.list_hunks {
        println!("Operation {} ({})", id, info.description);
        for file_diff in &info.file_diffs {
            println!("\nFile: {}", file_diff.file_path.display());
            for (i, hunk) in file_diff.hunks.iter().enumerate() {
                let range_text = if hunk.original_line_count == 1 {
                    format!("line {}", hunk.original_start_line)
                } else {
                    format!(
                        "lines {}–{}",
                        hunk.original_start_line,
                        hunk.original_start_line + hunk.original_line_count - 1
                    )
                };
                println!("  [Hunk {}] {}", i, range_text);

                // Show a preview of the hunk content if --preview is also used
                if undo_command.preview {
                    println!("    Original:");
                    for (idx, line) in hunk.original_lines.iter().enumerate() {
                        let line_num = hunk.original_start_line + idx;
                        println!("      {: >4}: {}", line_num, line);
                    }
                    println!("    Current:");
                    for (idx, line) in hunk.new_lines.iter().enumerate() {
                        let line_num = hunk.new_start_line + idx;
                        println!("      {: >4}: {}", line_num, line);
                    }
                }
            }
        }
        return Ok(());
    }

    // Handle preview of specific hunks
    if undo_command.preview {
        // Parse hunk indices if provided
        let hunk_indices: Vec<usize> = if let Some(hunks_str) = &undo_command.hunks {
            hunks_str
                .split(',')
                .map(|s| {
                    s.trim().parse::<usize>().map_err(|_| {
                        SearchError::config_error(format!("Invalid hunk index: {}", s.trim()))
                    })
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            // If no hunks specified, use all hunks
            (0..info.file_diffs.iter().map(|d| d.hunks.len()).sum()).collect()
        };

        for file_diff in &info.file_diffs {
            let file_path = file_diff.file_path.get_abs_path()?;
            let current_content = std::fs::read_to_string(&file_path)?;
            let mut preview_content = current_content.clone();

            // Apply selected hunks
            for &idx in &hunk_indices {
                if let Some(hunk) = file_diff.hunks.get(idx) {
                    // Apply hunk changes to preview_content
                    let lines: Vec<&str> = preview_content.lines().collect();
                    let mut new_lines = Vec::new();

                    // Copy lines before the hunk
                    new_lines.extend(lines.iter().take(hunk.new_start_line - 1).cloned());

                    // Add the original lines from the hunk
                    new_lines.extend(hunk.original_lines.iter().map(|s| s.as_str()));

                    // Copy remaining lines
                    new_lines.extend(
                        lines
                            .iter()
                            .skip(hunk.new_start_line - 1 + hunk.new_line_count)
                            .cloned(),
                    );

                    preview_content = new_lines.join("\n");
                }
            }

            print_unified_diff(&file_path, &current_content, &preview_content);
        }
        return Ok(());
    }

    // Handle --interactive
    if undo_command.interactive {
        match interactive_select_hunks(&info) {
            Ok(hunk_indices) => {
                if hunk_indices.is_empty() {
                    println!("No hunks selected. Operation cancelled.");
                    return Ok(());
                }
                ReplacementSet::undo_partial_by_id(id, &config, &hunk_indices)?;
                println!("Successfully reverted selected hunks.");
                return Ok(());
            }
            Err(e) => {
                println!("Interactive selection cancelled: {}", e);
                return Ok(());
            }
        }
    }

    // Parse hunk indices if provided
    let hunk_indices = if let Some(hunks_str) = &undo_command.hunks {
        hunks_str
            .split(',')
            .map(|s| {
                s.trim().parse::<usize>().map_err(|_| {
                    SearchError::config_error(format!("Invalid hunk index: {}", s.trim()))
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        // If no hunks specified, use all hunks
        (0..info.file_diffs.iter().map(|d| d.hunks.len()).sum()).collect()
    };

    // Confirm unless --force is used
    if !undo_command.force {
        print!("Are you sure you want to revert these changes? [y/N] ");
        std::io::stdout().flush()?;
        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;
        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Operation cancelled.");
            return Ok(());
        }
    }

    // Perform the actual revert
    if hunk_indices.is_empty() {
        ReplacementSet::undo_by_id(id, &config)?;
    } else {
        ReplacementSet::undo_partial_by_id(id, &config, &hunk_indices)?;
    }

    println!("Successfully reverted changes.");
    Ok(())
}

/// Handle workspace-related commands
fn handle_workspace(cmd: WorkspaceCommands) -> Result<()> {
    match cmd {
        WorkspaceCommands::Init(args) => {
            let dir = args.dir.unwrap_or_else(|| PathBuf::from("."));
            let format = args.format.to_lowercase();

            // Validate format
            if !["json", "yaml"].contains(&format.as_str()) {
                return Err(SearchError::config_error(
                    "Invalid format. Must be either 'json' or 'yaml'.",
                ));
            }

            // Validate directory
            let abs_dir = dir.canonicalize().map_err(|e| {
                SearchError::config_error(format!(
                    "Invalid directory path '{}': {}",
                    dir.display(),
                    e
                ))
            })?;

            // Check if directory exists and is a directory
            let metadata = std::fs::metadata(&abs_dir).map_err(|e| {
                SearchError::config_error(format!(
                    "Cannot read metadata for '{}': {}",
                    abs_dir.display(),
                    e
                ))
            })?;
            if !metadata.is_dir() {
                return Err(SearchError::config_error(format!(
                    "'{}' is not a directory",
                    abs_dir.display()
                )));
            }

            // Check write permissions with a test directory
            let test_path = abs_dir.join(".rustscout_write_test");
            match std::fs::create_dir(&test_path) {
                Ok(_) => {
                    std::fs::remove_dir(&test_path).ok(); // Cleanup
                }
                Err(e) => {
                    return Err(SearchError::config_error(format!(
                        "Directory '{}' is not writable: {}",
                        abs_dir.display(),
                        e
                    )));
                }
            }

            // Check for existing workspace
            let rustscout_dir = abs_dir.join(".rustscout");
            if rustscout_dir.exists() {
                if !args.force {
                    return Err(SearchError::config_error(format!(
                        "Directory '{}' is already a RustScout workspace. Use --force to reinitialize.",
                        abs_dir.display()
                    )));
                }
                println!(
                    "Warning: Reinitializing existing workspace at '{}'",
                    abs_dir.display()
                );
            }

            // Initialize workspace
            let workspace_root = rustscout::workspace::init_workspace(&abs_dir, &format)?;

            println!("Successfully initialized workspace:");
            println!("  Root: {}", workspace_root.root_path.display());
            println!("  Format: {}", workspace_root.format);
            println!(
                "  Config: {}",
                rustscout_dir
                    .join(if format == "yaml" {
                        "workspace.yaml"
                    } else {
                        "workspace.json"
                    })
                    .display()
            );

            Ok(())
        }
    }
}
