use clap::{Parser, Subcommand};
use colored::Colorize;
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::{EncodingMode, SearchConfig},
    errors::SearchError,
    replace::{PreviewResult, ReplacementConfig, ReplacementPattern, ReplacementSet, UndoInfo},
    results::SearchResult,
    search,
    search::matcher::{HyphenHandling, PatternDefinition, WordBoundaryMode},
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
        /// Pattern to search for (can be specified multiple times)
        #[arg(short = 'p', long = "pattern", action = clap::ArgAction::Append)]
        patterns: Vec<String>,

        /// Text to replace matches with (must match number of patterns)
        #[arg(short = 'r', long = "replacement", action = clap::ArgAction::Append)]
        replacements: Vec<String>,

        /// Treat patterns as regular expressions
        #[arg(long = "regex", action = clap::ArgAction::Append)]
        regex: Vec<bool>,

        /// Match whole words only
        #[arg(short = 'w', long = "word-boundary", action = clap::ArgAction::Append)]
        word_boundary: Vec<bool>,

        /// How to handle hyphens in word boundaries (boundary|joining)
        #[arg(long = "hyphen-handling", default_value = "joining")]
        hyphen_handling: String,

        /// Configuration file for replacements
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Dry run - show what would be changed without making changes
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Number of threads to use
        #[arg(short = 'j', long)]
        threads: Option<NonZeroUsize>,
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
                pattern_defs.push(PatternDefinition {
                    text: pattern.clone(),
                    is_regex: i < config.is_regex.len() && config.is_regex[i],
                    boundary_mode: if i < config.word_boundary.len() && config.word_boundary[i] {
                        WordBoundaryMode::WholeWords
                    } else {
                        WordBoundaryMode::None
                    },
                    hyphen_handling: HyphenHandling::default(),
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
            print_search_results(&result, config.stats);
            Ok(())
        }
        Commands::Replace {
            patterns,
            replacements,
            regex,
            word_boundary,
            hyphen_handling,
            config,
            dry_run,
            threads,
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

            // Validate patterns and replacements match
            if patterns.len() != replacements.len() {
                return Err(SearchError::config_error(
                    "Number of patterns must match number of replacements",
                ));
            }

            // Extend regex and word_boundary to match pattern count
            let regex_flags = if regex.is_empty() {
                vec![false; patterns.len()]
            } else if regex.len() == 1 {
                vec![regex[0]; patterns.len()]
            } else if regex.len() != patterns.len() {
                return Err(SearchError::config_error(
                    "Number of --regex flags must be 0, 1, or match number of patterns",
                ));
            } else {
                regex
            };

            let word_boundaries = if word_boundary.is_empty() {
                vec![false; patterns.len()]
            } else if word_boundary.len() == 1 {
                vec![word_boundary[0]; patterns.len()]
            } else if word_boundary.len() != patterns.len() {
                return Err(SearchError::config_error(
                    "Number of --word-boundary flags must be 0, 1, or match number of patterns",
                ));
            } else {
                word_boundary
            };

            // Parse hyphen handling
            let hyphen_mode = match hyphen_handling.as_str() {
                "boundary" => HyphenHandling::Boundary,
                "joining" => HyphenHandling::Joining,
                _ => return Err(SearchError::config_error("Invalid hyphen handling mode")),
            };

            // Create pattern definitions
            let mut pattern_defs = Vec::with_capacity(patterns.len());
            for i in 0..patterns.len() {
                let mut pattern_text = patterns[i].clone();
                if word_boundaries[i] && regex_flags[i] {
                    pattern_text = format!(r"\b{}\b", pattern_text);
                }

                pattern_defs.push(ReplacementPattern {
                    definition: PatternDefinition {
                        text: pattern_text,
                        is_regex: regex_flags[i],
                        boundary_mode: if word_boundaries[i] {
                            WordBoundaryMode::WholeWords
                        } else {
                            WordBoundaryMode::None
                        },
                        hyphen_handling: hyphen_mode,
                    },
                    replacement_text: replacements[i].clone(),
                });
            }

            repl_config.patterns = pattern_defs;

            // Set thread count if specified
            if let Some(thread_count) = threads {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(thread_count.get())
                    .build_global()
                    .map_err(|e| SearchError::config_error(e.to_string()))?;
            }

            // Create undo directory if it doesn't exist
            std::fs::create_dir_all(&repl_config.undo_dir).map_err(|e| {
                SearchError::config_error(format!("Failed to create undo dir: {}", e))
            })?;

            // Execute replacements
            let replacement_set = ReplacementSet::new(repl_config);
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
