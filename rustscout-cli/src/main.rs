use anyhow::Result;
use clap::Parser;
use colored::*;
use rustscout::search::search;
use rustscout::SearchConfig;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
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

    let env_filter = EnvFilter::from_default_env()
        .add_directive(level.into());

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

fn main() -> Result<()> {
    let args = Args::parse();

    // Load config file if it exists
    let config = if let Some(config_path) = args.config.as_deref() {
        SearchConfig::load_from(Some(config_path))?
    } else {
        SearchConfig::load().unwrap_or_else(|_| SearchConfig {
            pattern: args.pattern.clone(),
            root_path: args.root_path.clone(),
            file_extensions: args.extensions
                .as_ref()
                .map(|s| s.split(',').map(String::from).collect()),
            ignore_patterns: args.ignore.clone(),
            stats_only: args.stats_only,
            thread_count: NonZeroUsize::new(args.threads.unwrap_or_else(num_cpus::get))
                .expect("Thread count cannot be zero"),
            log_level: args.log_level.clone(),
        })
    };

    // Initialize logging with the configured level
    init_logging(&config.log_level)?;

    info!("Starting rustscout-cli with pattern: {}", config.pattern);

    // Set up thread pool if specified
    if config.thread_count.get() != num_cpus::get() {
        info!("Setting thread pool size to {}", config.thread_count);
        rayon::ThreadPoolBuilder::new()
            .num_threads(config.thread_count.get())
            .build_global()?;
    }

    info!("Searching with config: {:?}", config);
    let result = search(&config)?;

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
            }
        }

        println!(
            "\nTotal: {} matches in {} files",
            result.total_matches.to_string().green(),
            result.files_with_matches.to_string().green()
        );
    }

    Ok(())
}
