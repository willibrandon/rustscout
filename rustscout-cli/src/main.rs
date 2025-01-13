use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use rustscout::{Config, SearchResult, search};
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Search pattern (regular expression)
    pattern: String,

    /// Directory to search in
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Number of threads to use (default: number of CPU cores)
    #[arg(short, long)]
    threads: Option<NonZeroUsize>,

    /// Ignore files/directories matching pattern
    #[arg(short, long)]
    ignore: Option<String>,

    /// Show only summary statistics
    #[arg(short, long)]
    stats_only: bool,

    /// Comma-separated list of file extensions to search
    #[arg(short, long)]
    extensions: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config = Config {
        pattern: args.pattern,
        root_path: args.path,
        thread_count: args.threads.unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
        ignore_patterns: args.ignore.map(|p| vec![p]).unwrap_or_default(),
        file_extensions: args.extensions.map(|e| {
            e.split(',')
                .map(|s| s.trim().to_string())
                .collect()
        }),
        stats_only: args.stats_only,
    };

    let result = search::search(&config).context("Failed to perform search")?;

    if args.stats_only {
        print_stats(&result);
    } else {
        print_matches(&result);
    }

    Ok(())
}

fn print_matches(result: &SearchResult) {
    for file_result in &result.file_results {
        for m in &file_result.matches {
            let line_num = format!("{:>6}:", m.line_number).blue();
            println!("{}:{} {}", file_result.path.display(), line_num, m.line_content);
        }
    }

    print_stats(result);
}

fn print_stats(result: &SearchResult) {
    println!(
        "\nFound {} matches in {} files",
        result.total_matches.to_string().green(),
        result.files_with_matches.to_string().green()
    );
}
