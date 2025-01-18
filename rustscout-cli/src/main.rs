use clap::{Parser, Subcommand};
use colored::Colorize;
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    errors::SearchError,
    replace::{
        FileReplacementPlan, PreviewResult, ReplacementConfig, ReplacementPattern, ReplacementSet,
        ReplacementTask, UndoInfo,
    },
    results::SearchResult,
    search,
    search::matcher::{HyphenMode, PatternDefinition, WordBoundaryMode},
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

    /// Word boundary mode for the most recently specified pattern (strict|partial|none)
    #[arg(long = "boundary-mode", default_value = "none")]
    boundary_mode: String,

    /// Match whole words only (shorthand for --boundary-mode strict)
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode")]
    word_boundary: bool,

    /// How to handle hyphens in word boundaries (boundary|joining)
    #[arg(long = "hyphen-mode", default_value = "joining")]
    hyphen_mode: String,

    /// Root directory to search in
    #[arg(short = 'd', long, default_value = ".")]
    root: PathBuf,

    /// File extensions to include (e.g. rs,go,js)
    #[arg(short = 'e', long)]
    extensions: Option<String>,

    /// Patterns to ignore (glob format)
    #[arg(short, long)]
    ignore: Vec<String>,

    /// Number of context lines before match
    #[arg(short = 'B', long, default_value = "0")]
    context_before: usize,

    /// Number of context lines after match
    #[arg(short = 'A', long, default_value = "0")]
    context_after: usize,

    /// Show only statistics, not matches
    #[arg(short, long)]
    stats: bool,

    /// Number of threads to use
    #[arg(short = 'j', long)]
    threads: Option<NonZeroUsize>,

    /// Enable incremental search using cache
    #[arg(short = 'I', long)]
    incremental: bool,

    /// Path to cache file (default: .rustscout-cache.json)
    #[arg(long)]
    cache_path: Option<PathBuf>,

    /// Strategy for detecting file changes (auto|git|signature)
    #[arg(long, default_value = "auto")]
    cache_strategy: String,

    /// Maximum cache size in MB (0 for unlimited)
    #[arg(long)]
    max_cache_size: Option<u64>,

    /// Enable cache compression
    #[arg(long)]
    compress_cache: bool,

    /// How to handle invalid UTF-8 sequences (failfast|lossy)
    #[arg(long, default_value = "failfast")]
    encoding: String,

    /// Disable colored output
    #[arg(long = "no-color")]
    no_color: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Search for patterns in files
    Search(Box<CliSearchConfig>),

    /// Replace patterns in files
    Replace {
        /// Pattern to search for
        #[arg(short = 'p', long = "pattern", required = true)]
        pattern: String,

        /// Text to replace matches with
        #[arg(short = 'r', long = "replacement", required = true)]
        replacement: String,

        /// Treat patterns as regular expressions
        #[arg(short = 'R', long = "regex")]
        is_regex: bool,

        /// Word boundary mode (strict|partial|none)
        #[arg(long = "boundary-mode", default_value = "none")]
        boundary_mode: String,

        /// Match whole words only (shorthand for --boundary-mode strict)
        #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode")]
        word_boundary: bool,

        /// How to handle hyphens in word boundaries (boundary|joining)
        #[arg(long = "hyphen-mode", default_value = "joining")]
        hyphen_mode: String,

        /// Configuration file for replacements
        #[arg(short = 'c', long = "config")]
        config: Option<PathBuf>,

        /// Dry run - show what would be changed without making changes
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Number of threads to use
        #[arg(short = 'j', long)]
        threads: Option<NonZeroUsize>,

        /// One or more files/directories to process
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },

    /// List available undo operations
    ListUndo,

    /// Undo a previous replacement operation
    Undo {
        /// ID of the replacement to undo
        #[arg(required = true)]
        id: String,
    },
}

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
            print_search_results(&result, config.stats, config.no_color);
            Ok(())
        }
        Commands::Replace {
            pattern,
            replacement,
            is_regex,
            boundary_mode,
            word_boundary,
            hyphen_mode,
            config,
            dry_run,
            threads,
            paths,
        } => {
            // Load config file if provided
            let mut repl_config = if let Some(config_path) = config {
                ReplacementConfig::load_from(&config_path)?
            } else {
                ReplacementConfig {
                    patterns: vec![],
                    backup_enabled: true,
                    dry_run,
                    backup_dir: None,
                    preserve_metadata: true,
                    undo_dir: PathBuf::from(".rustscout").join("undo"),
                }
            };

            let target_paths = if paths.is_empty() {
                vec![PathBuf::from(".")] // Default to current directory if no paths provided
            } else {
                paths
            };

            // Create pattern definition
            let boundary_mode = if word_boundary {
                WordBoundaryMode::WholeWords
            } else {
                match boundary_mode.as_str() {
                    "strict" => WordBoundaryMode::WholeWords,
                    "partial" => WordBoundaryMode::Partial,
                    "none" => WordBoundaryMode::None,
                    _ => {
                        return Err(SearchError::config_error(format!(
                            "Invalid boundary mode '{}'. Valid values are: strict, partial, none",
                            boundary_mode
                        )))
                    }
                }
            };

            let pattern_def = PatternDefinition {
                text: pattern.clone(),
                is_regex,
                boundary_mode,
                hyphen_mode: match hyphen_mode.as_str() {
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
                replacement_text: replacement.clone(),
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
                thread_count: threads.unwrap_or_else(|| NonZeroUsize::new(4).unwrap()),
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
                                replacement.clone(),
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
                                replacement.clone(),
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
            if dry_run {
                let preview = replacement_set.preview()?;
                print_preview_results(&preview);
            } else {
                let _backups = replacement_set.apply_with_progress()?;
                print_replacement_results(&replacement_set, false);
            }

            Ok(())
        }
        Commands::ListUndo => {
            let config = ReplacementConfig::load_from(&PathBuf::from(".rustscout/config.json"))?;
            let operations = ReplacementSet::list_undo_operations(&config)?;
            print_undo_operations(&operations);
            Ok(())
        }
        Commands::Undo { id } => {
            let config = ReplacementConfig::load_from(&PathBuf::from(".rustscout/config.json"))?;
            let id = id
                .parse::<u64>()
                .map_err(|e| SearchError::config_error(format!("Invalid undo ID: {}", e)))?;
            ReplacementSet::undo_by_id(id, &config)?;
            println!("Successfully restored files from backup {}", id);
            Ok(())
        }
    }
}

fn print_search_results(result: &SearchResult, stats_only: bool, no_color: bool) {
    if stats_only {
        println!(
            "{} matches across {} files",
            result.total_matches, result.files_with_matches
        );
        return;
    }

    for file_result in &result.file_results {
        let file_path = file_result.path.display().to_string();
        
        // Group matches by line number
        let mut line_numbers_seen = std::collections::HashSet::new();
        
        for m in &file_result.matches {
            // Only print each line once
            if line_numbers_seen.insert(m.line_number) {
                // Print context before if this is the first time seeing this line
                for (line_num, line) in &m.context_before {
                    if !line_numbers_seen.contains(line_num) {
                        println!("{}:{}: {}", file_path, line_num, line);
                        line_numbers_seen.insert(*line_num);
                    }
                }
                
                // Print the matching line with highlighted matches
                let mut line = m.line_content.clone();
                let mut offset = 0;
                
                // Sort matches by start position to handle overlapping matches correctly
                let mut line_matches: Vec<_> = file_result.matches
                    .iter()
                    .filter(|other| other.line_number == m.line_number)
                    .collect();
                line_matches.sort_by_key(|m| m.start);
                
                // Apply highlighting to each match
                for other_match in line_matches {
                    let start = other_match.start + offset;
                    let end = other_match.end + offset;
                    let matched_text = line[start..end].to_string();
                    let highlighted = if no_color {
                        matched_text
                    } else {
                        format!("\x1b[1;31m{}\x1b[0m", matched_text)
                    };
                    line.replace_range(start..end, &highlighted);
                    offset += highlighted.len() - (end - start);
                }
                println!("{}:{}: {}", file_path, m.line_number, line);
                
                // Print context after
                for (line_num, line) in &m.context_after {
                    if !line_numbers_seen.contains(line_num) {
                        println!("{}:{}: {}", file_path, line_num, line);
                        line_numbers_seen.insert(*line_num);
                    }
                }
            }
        }
    }

    println!(
        "\n{} matches across {} files",
        result.total_matches, result.files_with_matches
    );
}

fn print_replacement_results(set: &ReplacementSet, dry_run: bool) {
    if dry_run {
        println!("Dry run - no changes will be made");
    }

    // Get preview of changes
    if let Ok(preview) = set.preview() {
        for result in preview {
            println!(
                "\nIn file: {}",
                result.file_path.display().to_string().blue()
            );
            for (i, (orig, new)) in result
                .original_lines
                .iter()
                .zip(result.new_lines.iter())
                .enumerate()
            {
                println!("Line {}: ", result.line_numbers[i]);
                println!("  - {}", orig.red());
                println!("  + {}", new.green());
            }
        }
    }

    // Print plan summary
    for plan in &set.plans {
        println!("\nIn file: {}", plan.file_path.display().to_string().blue());
        for replacement in &plan.replacements {
            println!(
                "Replace '{}' with '{}'",
                replacement.original_range.1.to_string().red(),
                replacement.replacement_text.green()
            );
        }
    }
}

fn print_undo_operations(operations: &[(UndoInfo, PathBuf)]) {
    if operations.is_empty() {
        println!("No undo operations available");
        return;
    }

    println!("Available undo operations:");
    for (info, path) in operations {
        println!("{}: {}", info.description, path.display());
    }
}

fn print_preview_results(preview: &[PreviewResult]) {
    for result in preview {
        println!("File: {}", result.file_path.display());
        for i in 0..result.original_lines.len() {
            println!("  - {}", result.original_lines[i]);
            println!("  + {}", result.new_lines[i]);
        }
        println!();
    }
}
