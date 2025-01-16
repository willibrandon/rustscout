use clap::{Parser, Subcommand};
use colored::Colorize;
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    replace::{ReplacementConfig, ReplacementSet, UndoInfo},
    results::SearchResult,
    search,
    search::matcher::{PatternDefinition, WordBoundaryMode, HyphenHandling},
    SearchError,
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

    /// Match whole words only for the most recently specified pattern
    #[arg(short = 'w', long = "word-boundary", action = clap::ArgAction::Append)]
    word_boundary: Vec<bool>,

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
}

#[derive(Subcommand)]
enum Commands {
    /// Search for patterns in files
    Search(Box<CliSearchConfig>),

    /// Replace patterns in files
    Replace {
        /// Pattern to search for
        #[arg(short = 'p', long = "pattern")]
        pattern: String,

        /// Replacement text
        #[arg(short = 'r', long = "replacement")]
        replacement: String,

        /// Treat the pattern as a regular expression
        #[arg(long)]
        regex: bool,

        /// Match whole words only
        #[arg(short = 'w', long = "word-boundary")]
        word_boundary: bool,

        /// Hyphen handling mode (boundary|joining)
        #[arg(long, default_value = "boundary")]
        hyphen_handling: String,

        /// Dry run - show what would be changed without making changes
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Number of threads to use
        #[arg(short = 'j', long)]
        threads: Option<NonZeroUsize>,

        /// Enable backups
        #[arg(long)]
        backup_enabled: bool,
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
            let mut patterns = config.patterns.clone();
            patterns.extend(config.legacy_patterns.iter().cloned());

            let mut pattern_defs = Vec::new();
            let mut is_regex = config.is_regex.clone();
            let mut word_boundary = config.word_boundary.clone();

            // Ensure we have enough flags for all patterns
            while is_regex.len() < patterns.len() {
                is_regex.push(false);
            }
            while word_boundary.len() < patterns.len() {
                word_boundary.push(false);
            }

            for (i, pattern) in patterns.iter().enumerate() {
                pattern_defs.push(PatternDefinition::new(
                    pattern.clone(),
                    is_regex[i],
                    if word_boundary[i] {
                        WordBoundaryMode::WholeWords
                    } else {
                        WordBoundaryMode::None
                    },
                ));
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
            print_search_results(&result, config.stats);
            Ok(())
        }
        Commands::Replace {
            pattern,
            replacement,
            regex,
            word_boundary,
            hyphen_handling,
            dry_run,
            threads: _,
            backup_enabled,
        } => {
            let hyphen_mode = match hyphen_handling.as_str() {
                "joining" => HyphenHandling::Joining,
                _ => HyphenHandling::Boundary,
            };

            let pattern_def = PatternDefinition {
                text: pattern,
                is_regex: regex,
                boundary_mode: if word_boundary {
                    WordBoundaryMode::WholeWords
                } else {
                    WordBoundaryMode::None
                },
                hyphen_handling: hyphen_mode,
                replacement: Some(replacement),
                capture_template: None,
            };

            let config = ReplacementConfig {
                patterns: vec![pattern_def],
                backup_enabled,
                dry_run,
                ..Default::default()
            };

            let set = ReplacementSet::new(config.clone());
            set.apply()?;
            print_replacement_results(&set, dry_run);
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

fn print_search_results(result: &SearchResult, stats_only: bool) {
    if stats_only {
        println!(
            "Found {} matches in {} files",
            result.total_matches, result.files_with_matches
        );
        return;
    }

    for file_result in &result.file_results {
        println!("\n{}", file_result.path.display().to_string().blue());
        for m in &file_result.matches {
            // Print context before
            for (line_num, line) in &m.context_before {
                println!("{}: {}", line_num.to_string().green(), line);
            }

            // Print match
            println!("{}: {}", m.line_number.to_string().green(), m.line_content);

            // Print context after
            for (line_num, line) in &m.context_after {
                println!("{}: {}", line_num.to_string().green(), line);
            }
        }
    }

    println!(
        "\nFound {} matches in {} files",
        result.total_matches, result.files_with_matches
    );
}

fn print_replacement_results(set: &ReplacementSet, dry_run: bool) {
    if dry_run {
        println!("Dry run - no changes will be made");
    }

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
