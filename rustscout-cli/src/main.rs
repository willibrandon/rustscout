use clap::{Parser, Subcommand};
use colored::Colorize;
use rustscout::{
    cache::ChangeDetectionStrategy,
    config::SearchConfig,
    errors::SearchError,
    replace::{ReplacementConfig, ReplacementSet, UndoInfo},
    results::SearchResult,
    search,
};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search for patterns in files
    Search {
        /// Search pattern(s) to use
        #[arg(required = true)]
        patterns: Vec<String>,

        /// Root directory to search in
        #[arg(short, long, default_value = ".")]
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
        #[arg(short = 'i', long)]
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
    },

    /// Replace patterns in files
    Replace {
        /// Configuration file for replacements
        #[arg(short, long)]
        config: PathBuf,

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

fn main() -> Result<(), SearchError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Search {
            patterns,
            root,
            extensions,
            ignore,
            stats,
            threads,
            context_before,
            context_after,
            incremental,
            cache_path,
            cache_strategy,
            max_cache_size,
            compress_cache,
        } => {
            let file_extensions = extensions.map(|e| {
                e.split(',')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<_>>()
            });

            let cache_strategy = match cache_strategy.as_str() {
                "git" => ChangeDetectionStrategy::GitStatus,
                "signature" => ChangeDetectionStrategy::FileSignature,
                _ => ChangeDetectionStrategy::Auto,
            };

            let config = SearchConfig {
                patterns,
                pattern: String::new(),
                root_path: root,
                file_extensions,
                ignore_patterns: ignore,
                stats_only: stats,
                thread_count: threads.unwrap_or_else(|| NonZeroUsize::new(4).unwrap()),
                log_level: "info".to_string(),
                context_before,
                context_after,
                incremental,
                cache_path,
                cache_strategy,
                max_cache_size: max_cache_size.map(|size| size * 1024 * 1024),
                use_compression: compress_cache,
            };

            let result = search(&config)?;
            print_search_results(&result, stats);
            Ok(())
        }
        Commands::Replace {
            config,
            dry_run,
            threads: _,
        } => {
            let config = ReplacementConfig::load_from(&config)?;
            let set = ReplacementSet::new(config.clone());
            set.apply()?;
            print_replacement_results(&set, dry_run);
            Ok(())
        }
        Commands::ListUndo => {
            let config = ReplacementConfig::load_from(Path::new(".rustscout/config.json"))?;
            let operations = ReplacementSet::list_undo_operations(&config)?;
            print_undo_operations(&operations);
            Ok(())
        }
        Commands::Undo { id } => {
            let config = ReplacementConfig::load_from(Path::new(".rustscout/config.json"))?;
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
