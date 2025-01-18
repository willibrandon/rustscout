use clap::{Parser, Subcommand};
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    errors::SearchError,
    replace::{
        FileReplacementPlan, ReplacementConfig, ReplacementPattern, ReplacementSet, ReplacementTask,
    },
    search,
    search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode},
    Match,
};
use std::{num::NonZeroUsize, path::PathBuf};

type Result<T> = std::result::Result<T, SearchError>;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
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
}

#[derive(Subcommand)]
enum Commands {
    /// Search for patterns in files
    Search(Box<CliSearchConfig>),

    /// Replace patterns in files
    Replace {
        #[command(subcommand)]
        command: ReplaceCommands,
    },
}

#[derive(Subcommand)]
enum ReplaceCommands {
    /// Perform a search/replace operation
    Do(ReplaceDo),

    /// Undo a previous replacement operation
    Undo(ReplaceUndo),
}

#[derive(Parser)]
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

#[derive(Parser)]
struct ReplaceUndo {
    /// ID of the replacement to undo
    #[arg()]
    id: String,
}

mod diff_utils;
use diff_utils::{print_side_by_side_diff, print_unified_diff};

fn main() -> Result<()> {
    run()
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Search(config) => {
            let mut pattern_defs = Vec::new();

            // Convert CLI patterns to pattern definitions
            for (i, pattern) in config
                .patterns
                .iter()
                .chain(config.legacy_patterns.iter())
                .enumerate()
            {
                let boundary_mode = if config.word_boundary {
                    WordBoundaryMode::WholeWords
                } else {
                    match config.boundary_mode.as_str() {
                        "strict" => WordBoundaryMode::WholeWords,
                        "partial" => WordBoundaryMode::Partial,
                        "none" => WordBoundaryMode::None,
                        _ => {
                            return Err(SearchError::config_error(format!(
                            "Invalid boundary mode '{}'. Valid values are: strict, partial, none",
                            config.boundary_mode
                        )))
                        }
                    }
                };

                pattern_defs.push(PatternDefinition {
                    text: pattern.clone(),
                    is_regex: i < config.is_regex.len() && config.is_regex[i],
                    boundary_mode,
                    hyphen_mode: match config.hyphen_mode.as_str() {
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

            let file_extensions = config.extensions.as_ref().map(|e| {
                e.split(',')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<_>>()
            });

            let cache_strategy = match config.cache_strategy.as_str() {
                "git" => ChangeDetectionStrategy::GitStatus,
                "signature" => ChangeDetectionStrategy::FileSignature,
                _ => ChangeDetectionStrategy::Auto,
            };

            let encoding_mode = match config.encoding.to_lowercase().as_str() {
                "lossy" => EncodingMode::Lossy,
                _ => EncodingMode::FailFast,
            };

            let search_config = SearchConfig {
                pattern_definitions: pattern_defs,
                root_path: config.root,
                file_extensions,
                ignore_patterns: config.ignore,
                stats_only: config.stats,
                thread_count: config
                    .threads
                    .unwrap_or_else(|| NonZeroUsize::new(4).unwrap()),
                log_level: "info".to_string(),
                context_before: config.context_before,
                context_after: config.context_after,
                incremental: config.incremental,
                cache_path: config.cache_path,
                cache_strategy,
                max_cache_size: config.max_cache_size.map(|size| size * 1024 * 1024),
                use_compression: config.compress_cache,
                encoding_mode,
            };

            let result = search(&search_config)?;

            if config.stats {
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
                    for ctx_line_num in (line_num.saturating_sub(config.context_before))..line_num {
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
                            if config.no_color {
                                highlighted_line.push_str(&line[m.start..m.end]);
                            } else {
                                highlighted_line.push_str(&format!(
                                    "\x1b[1;31m{}\x1b[0m",
                                    &line[m.start..m.end]
                                ));
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
                    let end_ctx = (line_num + config.context_after).min(all_lines.len());
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
        Commands::Replace { command } => {
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
                            let search_result = search(&SearchConfig {
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
                            let search_result = search(&SearchConfig {
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
                            "unified" => {
                                print_unified_diff(&plan.file_path, &old_content, &new_content)
                            }
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
                ReplaceCommands::Undo(undo_command) => {
                    let config =
                        ReplacementConfig::load_from(&PathBuf::from(".rustscout/config.json"))?;
                    let id = undo_command.id.parse::<u64>().map_err(|e| {
                        SearchError::config_error(format!("Invalid undo ID: {}", e))
                    })?;
                    ReplacementSet::undo_by_id(id, &config)?;
                    println!("Successfully restored files from backup {}", id);
                    Ok(())
                }
            }
        }
    }
}
