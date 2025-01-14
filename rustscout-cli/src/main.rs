use anyhow::{anyhow, Result};
use clap::{ArgMatches, Command, CommandFactory, Parser, Subcommand};
use colored::{Colorize, *};
use rustscout::search::search;
use rustscout::{
    FileReplacementPlan, ReplacementConfig, ReplacementSet, ReplacementTask, SearchConfig,
};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::{fmt, EnvFilter};
use std::fs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search for patterns in files
    Search {
        /// Pattern to search for (supports regex)
        pattern: String,

        /// Root directory to search in
        #[arg(default_value = ".")]
        root_path: PathBuf,

        /// Comma-separated list of file extensions to search (e.g. "rs,toml")
        #[arg(short, long)]
        extensions: Option<String>,

        /// Additional patterns to ignore (supports .gitignore syntax)
        #[arg(short, long)]
        ignore: Vec<String>,

        /// Show only statistics, not individual matches
        #[arg(long)]
        stats_only: bool,

        /// Number of threads to use for searching (default: number of CPU cores)
        #[arg(short, long)]
        threads: Option<usize>,

        /// Log level (trace, debug, info, warn, error)
        #[arg(short, long, default_value = "warn")]
        log_level: String,

        /// Path to config file (default: .rustscout.yaml)
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Number of context lines to show before each match
        #[arg(short = 'B', long, default_value = "0")]
        context_before: usize,

        /// Number of context lines to show after each match
        #[arg(short = 'A', long, default_value = "0")]
        context_after: usize,

        /// Number of context lines to show before and after each match
        #[arg(short = 'C', long)]
        context: Option<usize>,
    },

    /// Search and replace patterns in files
    Replace {
        /// Pattern to search for (supports regex)
        pattern: String,

        /// Files or directories to process
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// The replacement text (for simple text)
        #[arg(short = 'r', long = "replace")]
        replacement: Option<String>,

        /// The regex pattern to search for
        #[arg(short = 'R', long = "regex")]
        regex_pattern: Option<String>,

        /// Use capture groups in the replacement (e.g. "$1, $2")
        #[arg(short = 'g', long = "capture-groups")]
        capture_groups: Option<String>,

        /// Show what would be changed, but don't modify files
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Create backups of modified files
        #[arg(short, long)]
        backup: bool,

        /// Directory for backups / temp files
        #[arg(short = 'o', long = "output-dir")]
        backup_dir: Option<PathBuf>,

        /// Load additional config
        #[arg(short = 'f', long = "config-file")]
        config_file: Option<PathBuf>,

        /// Show detailed preview of changes
        #[arg(short = 'p', long)]
        preview: bool,

        /// Preserve file permissions and timestamps
        #[arg(long)]
        preserve: bool,

        /// Additional patterns to ignore (supports .gitignore syntax)
        #[arg(short, long)]
        ignore: Vec<String>,

        /// Number of threads to use (default: number of CPU cores)
        #[arg(short, long)]
        threads: Option<usize>,

        /// Log level (trace, debug, info, warn, error)
        #[arg(short, long, default_value = "warn")]
        log_level: String,
    },

    /// List available undo operations
    ListUndo,

    /// Undo a previous replacement operation
    Undo {
        /// ID of the operation to undo (from list-undo)
        id: u64,
    },
}

fn init_logging(level: &str) -> Result<()> {
    let level = match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::WARN,
    };

    let env_filter = EnvFilter::from_default_env().add_directive(level.into());

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("Logging initialized at level: {}", level);
    Ok(())
}

fn run_list_undo(args: &Args) -> Result<()> {
    let config = ReplacementConfig {
        pattern: String::new(),
        replacement: String::new(),
        is_regex: false,
        backup_enabled: false,
        dry_run: false,
        backup_dir: None,
        preserve_metadata: false,
        capture_groups: None,
        undo_dir: PathBuf::from(".rustscout/undo"),
    };

    let operations = ReplacementSet::list_undo_operations(&config)?;
    if operations.is_empty() {
        println!("No undo operations available");
        return Ok(());
    }

    println!("Available undo operations:");
    for (i, (info, _)) in operations.iter().enumerate() {
        println!("[{}] {}", i.to_string().yellow(), info);
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    match &args.command {
        Commands::Search {
            pattern,
            root_path,
            extensions,
            ignore,
            stats_only,
            threads,
            log_level,
            config,
            context_before,
            context_after,
            context,
        } => {
            init_logging(log_level)?;

            // Set up thread pool if specified
            if let Some(threads) = threads {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(*threads)
                    .build_global()?;
            }

            // Create search config
            let config = if let Some(config_path) = config.as_deref() {
                SearchConfig::load_from(Some(config_path))?
            } else {
                SearchConfig {
                    patterns: vec![pattern.clone()],
                    pattern: pattern.to_string(),
                    root_path: root_path.to_path_buf(),
                    file_extensions: extensions
                        .as_ref()
                        .map(|s| s.split(',').map(String::from).collect()),
                    ignore_patterns: ignore.to_vec(),
                    stats_only: *stats_only,
                    thread_count: NonZeroUsize::new(threads.map(|t| t).unwrap_or_else(num_cpus::get))
                        .expect("Thread count cannot be zero"),
                    log_level: log_level.to_string(),
                    context_before: context.map(|c| c).unwrap_or(*context_before),
                    context_after: context.map(|c| c).unwrap_or(*context_after),
                }
            };

            // Perform search
            let result = search(&config)?;

            // Display results
            if config.stats_only {
                println!(
                    "Found {} matches in {} files",
                    result.total_matches.to_string().green(),
                    result.files_with_matches.to_string().green()
                );
            } else {
                for file_result in result.file_results {
                    println!(
                        "\n{}: {} matches",
                        file_result.path.display().to_string().blue(),
                        file_result.matches.len().to_string().green()
                    );

                    for m in file_result.matches {
                        // Print context before
                        for (line_num, line) in &m.context_before {
                            println!("{}: {}", line_num.to_string().yellow(), line);
                        }

                        // Print the match
                        let line_content = m.line_content.trim();
                        let before = &line_content[..m.start];
                        let matched = &line_content[m.start..m.end];
                        let after = &line_content[m.end..];

                        println!(
                            "{}: {}{}{}",
                            m.line_number.to_string().yellow(),
                            before,
                            matched.red(),
                            after
                        );

                        // Print context after
                        for (line_num, line) in &m.context_after {
                            println!("{}: {}", line_num.to_string().yellow(), line);
                        }

                        // Print separator between matches if there are context lines
                        if !m.context_before.is_empty() || !m.context_after.is_empty() {
                            println!("--");
                        }
                    }
                }

                println!(
                    "\nTotal: {} matches in {} files",
                    result.total_matches.to_string().green(),
                    result.files_with_matches.to_string().green()
                );
            }
        }

        Commands::Replace {
            pattern,
            files,
            replacement,
            regex_pattern,
            capture_groups,
            dry_run,
            backup,
            backup_dir,
            config_file,
            preview,
            preserve,
            ignore: _,
            threads,
            log_level,
        } => {
            init_logging(log_level)?;

            // Set up thread pool if specified
            if let Some(threads) = threads {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(*threads)
                    .build_global()?;
            }

            let mut config = if let Some(path) = config_file {
                ReplacementConfig::load_from(&path)?
            } else {
                ReplacementConfig {
                    pattern: pattern.clone(),
                    replacement: replacement
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    is_regex: regex_pattern.is_some(),
                    backup_enabled: *backup,
                    dry_run: *dry_run,
                    backup_dir: backup_dir.clone(),
                    preserve_metadata: *preserve,
                    capture_groups: capture_groups.clone(),
                    undo_dir: PathBuf::from(".rustscout/undo"),
                }
            };

            // CLI options take precedence over config file
            config.merge_with_cli(ReplacementConfig {
                pattern: pattern.clone(),
                replacement: replacement
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                is_regex: regex_pattern.is_some(),
                backup_enabled: *backup,
                dry_run: *dry_run,
                backup_dir: backup_dir.clone(),
                preserve_metadata: *preserve,
                capture_groups: capture_groups.clone(),
                undo_dir: PathBuf::from(".rustscout/undo"),
            });

            let search_pattern = regex_pattern.as_ref().unwrap_or(&pattern).clone();

            let search_config = SearchConfig {
                patterns: vec![search_pattern.clone()],
                pattern: search_pattern,
                root_path: files[0].clone(),
                file_extensions: None,
                ignore_patterns: vec![],
                stats_only: false,
                thread_count: NonZeroUsize::new(threads.unwrap_or_else(num_cpus::get))
                    .expect("Thread count cannot be zero"),
                log_level: "warn".to_string(),
                context_before: 0,
                context_after: 0,
            };

            // Perform search
            let result = search(&search_config)?;

            let mut replacement_set = ReplacementSet::new(config.clone());

            // Create replacement plans from search results
            for file_result in &result.file_results {
                let mut plan = FileReplacementPlan::new(file_result.path.clone())?;

                for m in &file_result.matches {
                    plan.add_replacement(ReplacementTask::new(
                        file_result.path.clone(),
                        (m.start, m.end),
                        replacement
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default(),
                        config.clone(),
                    ));
                }

                replacement_set.add_plan(plan);
            }

            // Show preview if requested
            if *preview {
                println!("\nPreview of changes:");
                for preview in replacement_set.preview()? {
                    println!(
                        "\n{}: {} changes",
                        preview.file_path.display().to_string().blue(),
                        preview.original_lines.len().to_string().green()
                    );

                    for i in 0..preview.original_lines.len() {
                        println!(
                            "{}: {}",
                            preview.line_numbers[i].to_string().yellow(),
                            preview.original_lines[i].red()
                        );
                        println!(
                            "{}: {}",
                            preview.line_numbers[i].to_string().yellow(),
                            preview.new_lines[i].green()
                        );
                        println!("--");
                    }
                }

                if !*dry_run {
                    print!("Apply these changes? [y/N] ");
                    std::io::Write::flush(&mut std::io::stdout())?;
                    let mut response = String::new();
                    std::io::stdin().read_line(&mut response)?;
                    if !response.trim().eq_ignore_ascii_case("y") {
                        println!("Aborting.");
                        return Ok(());
                    }
                }
            }

            // Apply replacements with progress reporting
            if *dry_run {
                println!("\nDry run - no files will be modified");
            }
            let undo_metadata = replacement_set.apply_with_progress()?;

            println!(
                "\nReplaced {} occurrences in {} files",
                result.total_matches.to_string().green(),
                result.files_with_matches.to_string().green()
            );

            if !*dry_run && !undo_metadata.is_empty() {
                println!("\nTo undo these changes later, use:");
                println!("  rustscout list-undo    # to see available undo operations");
                println!("  rustscout undo <id>    # to undo this operation");
            }
        }

        Commands::ListUndo => {
            run_list_undo(&args)?;
        }

        Commands::Undo { id } => {
            println!("Undoing operation {}...", id);
            let undo_dir = PathBuf::from(".rustscout/undo");
            fs::create_dir_all(&undo_dir).map_err(|e| anyhow!("Failed to create undo directory: {}", e))?;
            
            let config = ReplacementConfig {
                pattern: String::new(),
                replacement: String::new(),
                is_regex: false,
                backup_enabled: false,
                dry_run: false,
                backup_dir: None,
                preserve_metadata: false,
                capture_groups: None,
                undo_dir,
            };
            ReplacementSet::undo_by_id(*id, &config)?;
            println!("Undo complete");
        }
    }

    Ok(())
}
