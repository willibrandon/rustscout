use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use rustscout::{Config, SearchResult};
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The pattern to search for (regex)
    pattern: String,

    /// The root directory to search in
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Number of threads to use (default: number of CPU cores)
    #[arg(short, long)]
    threads: Option<NonZeroUsize>,

    /// File patterns to ignore (e.g. "*.git/*")
    #[arg(short, long)]
    ignore: Vec<String>,

    /// Only show statistics, not matches
    #[arg(short, long)]
    stats_only: bool,

    /// File extensions to include (e.g. "rs,txt")
    #[arg(short = 'e', long)]
    extensions: Option<String>,
}

fn print_match(result: &SearchResult) {
    for file_result in &result.file_results {
        if !file_result.matches.is_empty() {
            println!("\n{}", file_result.path.display().to_string().green());

            for m in &file_result.matches {
                let line_num = format!("{:>6}:", m.line_number).blue();
                let before = &m.line_content[..m.start];
                let matched = &m.line_content[m.start..m.end];
                let after = &m.line_content[m.end..];

                println!("{} {}{}{}", line_num, before, matched.red(), after);
            }
        }
    }
}

fn print_stats(result: &SearchResult) {
    println!("\nSearch Statistics:");
    println!("  Files searched: {}", result.files_searched);
    println!("  Files with matches: {}", result.files_with_matches);
    println!("  Total matches: {}", result.total_matches);
}

fn main() -> Result<()> {
    let args = Args::parse();

    let extensions = args.extensions.map(|e| {
        e.split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    });

    let config = Config::new(args.pattern, args.path)
        .with_ignore_patterns(args.ignore)
        .with_stats_only(args.stats_only);

    let config = if let Some(threads) = args.threads {
        config.with_thread_count(threads)
    } else {
        config
    };

    let config = if let Some(exts) = extensions {
        config.with_file_extensions(exts)
    } else {
        config
    };

    let result = rtrace_core::search::search(&config).context("Failed to perform search")?;

    if !args.stats_only {
        print_match(&result);
    }
    print_stats(&result);

    Ok(())
}
