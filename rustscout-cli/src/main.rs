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
#[command(name = "RustScout CLI")]
#[command(about = "RustScout is a **high-performance** code search and replace tool, featuring concurrency, incremental caching, partial/interactive workflows, and robust workspace management.")]
#[command(long_about = None)]
#[command(after_help = "\
Global Options:
  -h, --help        Print this help message
  -V, --version     Print the version of RustScout
  -v, --verbosity <LEVEL>
                    Set the global log level (error|warn|info|debug|trace)
                    (Defaults to 'info')

Commands:
  search (s)               High-speed, multi-pattern code search with boundary
                           modes, incremental caching, and rich output
  replace (r)              Full-featured find-and-replace with undo, partial
                           revert, and interactive approvals
  interactive-search (i)   Steer through matches one by one in a TUI, optionally
                           editing them in place
  workspace (w)            Initialize and manage RustScout's workspace metadata
  help (h)                 Display help or usage for any command

For detailed usage of each command, run:
  rustscout-cli <COMMAND> --help

Examples:
  # Basic code search
  rustscout-cli search -p \"TODO\" -d ./src
  
  # Perform a replacement operation with backup and interactive confirmations
  rustscout-cli replace do --pattern foo --replacement bar -B --interactive
  
  # Undo the last replacement by its ID
  rustscout-cli replace undo 1672834920
  
  # Launch TUI to step through each match, showing context
  rustscout-cli interactive-search -p \"fixme\" -B 2 -A 2
  
  # Initialize a new RustScout workspace at /my_project
  rustscout-cli workspace init --dir /my_project")]
struct Cli {
    /// Set the global log level (error|warn|info|debug|trace)
    #[arg(short = 'v', long = "verbosity", global = true, default_value = "info")]
    verbosity: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// High-speed, multi-pattern code search with boundary modes, incremental caching, and rich output
    #[command(visible_alias = "s")]
    Search(Box<CliSearchConfig>),

    /// Full-featured find-and-replace with undo, partial revert, and interactive approvals
    #[command(visible_alias = "r")]
    Replace {
        #[command(subcommand)]
        command: ReplaceCommands,
    },

    /// Steer through matches one by one in a TUI, optionally editing them in place
    #[command(visible_alias = "i")]
    InteractiveSearch(Box<InteractiveSearchArgs>),

    /// Initialize and manage RustScout's workspace metadata
    #[command(visible_alias = "w")]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
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

    /// Undo or partially revert a previous replacement operation
    Undo(ReplaceUndo),
}

#[derive(Parser, Debug)]
#[command(about = "Powerfully search your codebase for patterns—literal or regex—while leveraging features like boundary modes, incremental caching, context lines, and more")]
#[command(after_help = "\
Example Workflows:

1. Simple Literal Search
   rustscout-cli search -p \"TODO\" -d ./src
   Finds \"TODO\" in the src directory.

2. Multiple Patterns (Mixed Regex/Literal)
   rustscout-cli search \\
     -p \"MyClass\" -r false \\
     -p \"fn (\\w+)\\(\" -r true \\
     -B 2 -A 2
   Searches for both a literal \"MyClass\" and a Rust function definition pattern, showing 2 lines of surrounding context.

3. Incremental Caching for Speed
   rustscout-cli search \\
     -p \"RefactorMe\" \\
     -I --cache-path .rustscout-cache.json \\
     -S git \\
     -j 8
   Uses 8 threads, leverages Git metadata for incremental scanning, and caches results in .rustscout-cache.json.

4. File Extension & Ignore Patterns
   rustscout-cli search \\
     -p \"TODO\" \\
     -x rs,ts \\
     -g \"**/dist/**\" \\
     --stats
   Only searches .rs or .ts files, ignores the dist/ folder, and prints statistics (e.g., how many matches found).

5. Handling Special UTF-8 Cases
   rustscout-cli search \\
     -p \"🔑KEYWORD\" \\
     -E lossy
   Continues searching in files even if some contain invalid UTF-8 sequences, representing them with placeholders.")]
struct CliSearchConfig {
    /// Specifies a pattern to search for. Can be provided multiple times:
    /// Each -p adds a new pattern. By default, these are literal substring matches.
    #[arg(short = 'p', long = "pattern", help_heading = "Core Pattern Options")]
    patterns: Vec<String>,

    /// Legacy positional patterns (deprecated)
    #[arg(hide = true)]
    legacy_patterns: Vec<String>,

    /// For the most recently specified --pattern, treat it as a regular expression (if true).
    /// Example:
    ///   rustscout-cli search -p "fn (\w+)\(\)" -r true
    /// This matches Rust function definitions.
    /// Tip: You can set -r false again if you want the next pattern to be literal.
    #[arg(short = 'r', long = "regex", action = clap::ArgAction::Append, help_heading = "Core Pattern Options")]
    is_regex: Vec<bool>,

    /// Specifies word boundary handling:
    /// - strict: Only match whole words
    /// - partial: Loose boundary detection
    /// - none (default): No boundary constraints
    /// Tip: Use -w, --word-boundary as a shorthand for --boundary-mode strict
    #[arg(short = 'b', long = "boundary-mode", default_value = "none", help_heading = "Core Pattern Options")]
    boundary_mode: String,

    /// Shorthand for --boundary-mode strict
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode", help_heading = "Core Pattern Options")]
    word_boundary: bool,

    /// Determines how hyphens are treated in word boundaries:
    /// - boundary: Hyphens are considered separate boundaries
    /// - joining (default): Hyphens are treated as word characters, bridging word parts
    #[arg(short = 'y', long = "hyphen-mode", default_value = "joining", help_heading = "Core Pattern Options")]
    hyphen_mode: String,

    /// Specifies the root directory to search in.
    /// Default: Current directory (.)
    #[arg(short = 'd', long = "root", default_value = ".", help_heading = "File/Directory Options")]
    root: PathBuf,

    /// Comma-separated list of file extensions to include.
    /// Example: -x rs,go,js
    #[arg(short = 'x', long = "extensions", help_heading = "File/Directory Options")]
    extensions: Option<String>,

    /// Defines ignore patterns (in glob format) for files or directories.
    /// Example: -g "**/node_modules/**" to skip node modules.
    #[arg(short = 'g', long = "ignore", help_heading = "File/Directory Options")]
    ignore: Vec<String>,

    /// Number of context lines before each match (default: 0)
    #[arg(short = 'B', long = "context-before", default_value = "0", help_heading = "Match Output & Context")]
    context_before: usize,

    /// Number of context lines after each match (default: 0)
    /// When used together (like -B 2 -A 2), you get a small snippet of lines around each match—helpful for code review.
    #[arg(short = 'A', long = "context-after", default_value = "0", help_heading = "Match Output & Context")]
    context_after: usize,

    /// Show only statistics, not the actual matches.
    /// Perfect for counting how many files or lines matched without spamming the terminal.
    #[arg(short = 's', long = "stats", help_heading = "Match Output & Context")]
    stats: bool,

    /// Number of threads to use for parallel searching.
    /// Defaults to the number of CPU cores.
    #[arg(short = 'j', long = "threads", help_heading = "Performance & Caching")]
    threads: Option<NonZeroUsize>,

    /// Enable incremental search using a local cache of file checksums or Git metadata. This speeds up repeated searches.
    /// Combine with -C, -S, -M, -Z for advanced tuning.
    #[arg(short = 'I', long = "incremental", help_heading = "Performance & Caching")]
    incremental: bool,

    /// Specifies the path to the cache file (default: .rustscout-cache.json)
    #[arg(short = 'C', long = "cache-path", help_heading = "Performance & Caching")]
    cache_path: Option<PathBuf>,

    /// Sets the strategy for detecting changed files:
    /// - auto (default): Heuristics based on modification times, file size, etc.
    /// - git: Use Git's index or HEAD references (when in a Git repo)
    /// - signature: Compute checksums or signatures
    #[arg(short = 'S', long = "cache-strategy", default_value = "auto", help_heading = "Performance & Caching")]
    cache_strategy: String,

    /// Limits the cache to <MB> megabytes. Use 0 for unlimited.
    #[arg(short = 'M', long = "max-cache-size", help_heading = "Performance & Caching")]
    max_cache_size: Option<u64>,

    /// Enables compression for the incremental cache. Useful for large codebases with limited disk space.
    #[arg(short = 'Z', long = "compress-cache", help_heading = "Performance & Caching")]
    compress_cache: bool,

    /// Controls how to handle invalid UTF-8 sequences:
    /// - failfast (default): Abort on invalid sequences
    /// - lossy: Replace invalid bytes with placeholders, continuing the search
    #[arg(short = 'E', long = "encoding", default_value = "failfast", help_heading = "Miscellaneous")]
    encoding: String,

    /// Disables colored output. Handy for scripts or logs that don't support ANSI colors.
    #[arg(short = 'N', long = "no-color", help_heading = "Miscellaneous")]
    no_color: bool,
}

/// Perform a powerful, configurable search‐and‐replace across multiple files or directories, with optional backups, interactive TUI, and advanced pattern matching.
#[derive(Parser, Debug)]
#[command(about = "Perform a search/replace operation across one or more files/directories")]
#[command(long_about = "Perform a powerful, configurable search‐and‐replace across multiple files or directories, with optional backups, interactive TUI, and advanced pattern matching.")]
#[command(after_help = "\
Examples:
  # Simple literal replace
  rustscout-cli replace do -p foo -r bar src/**/*.rs

  # Regex replacement with capture groups
  rustscout-cli replace do -x --pattern 'fn (\\w+)\\(\\)' --replacement 'fn new_$1()' src/**/*.rs

  # Preview with side-by-side diffs
  rustscout-cli replace do -p HTTP -r HTTPS -n --diff-format side-by-side /var/www

  # Interactive approval with backups
  rustscout-cli replace do --pattern temp --replacement permanent --interactive --backup .")]
struct ReplaceDo {
    /// Text or pattern to search for
    #[arg(short = 'p', long = "pattern", required = true, value_name = "PATTERN")]
    #[arg(help_heading = "Required Options")]
    pattern: String,

    /// Text to replace matches with
    #[arg(short = 'r', long = "replacement", required = true, value_name = "REPLACEMENT")]
    #[arg(help_heading = "Required Options")]
    replacement: String,

    /// Treat pattern as a regular expression
    #[arg(short = 'x', long = "regex")]
    #[arg(help_heading = "General Options")]
    is_regex: bool,

    /// Word boundary handling for matches:
    /// - none (default) – match anywhere
    /// - partial – partial boundary detection
    /// - strict – only match whole words
    #[arg(short = 'b', long = "boundary-mode", default_value = "none", value_name = "MODE")]
    #[arg(help_heading = "General Options")]
    boundary_mode: String,

    /// Shorthand for --boundary-mode strict
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode")]
    #[arg(help_heading = "General Options")]
    word_boundary: bool,

    /// How to treat hyphens in boundary detection (boundary|joining)
    #[arg(short = 'y', long = "hyphen-mode", default_value = "joining", value_name = "MODE")]
    #[arg(help_heading = "General Options")]
    hyphen_mode: String,

    /// Load advanced configuration from a YAML/JSON file (e.g., multiple patterns, filtering rules)
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    #[arg(help_heading = "General Options")]
    config: Option<PathBuf>,

    /// Shows what would be changed without modifying files. Great for previews
    #[arg(short = 'n', long = "dry-run")]
    #[arg(help_heading = "General Options")]
    dry_run: bool,

    /// Format of diffs shown in a dry run (unified|side-by-side)
    #[arg(short = 'd', long = "diff-format", default_value = "unified", value_name = "FORMAT")]
    #[arg(help_heading = "General Options")]
    diff_format: String,

    /// Number of threads to use (default: CPU cores)
    #[arg(short = 'j', long = "threads", value_name = "N")]
    #[arg(help_heading = "General Options")]
    threads: Option<NonZeroUsize>,

    /// Opens an interactive "approve or skip" TUI for each match. Perfect for selectively replacing matches in large codebases
    #[arg(short = 'i', long = "interactive")]
    #[arg(help_heading = "Advanced Options")]
    interactive: bool,

    /// Creates backups in .rustscout/undo for each changed file, enabling an easy revert with replace undo
    #[arg(short = 'B', long = "backup")]
    #[arg(help_heading = "Advanced Options")]
    backup: bool,

    /// Keeps file permissions and timestamps intact after replacement
    #[arg(short = 'm', long = "preserve-metadata")]
    #[arg(help_heading = "Advanced Options")]
    preserve_metadata: bool,

    /// Additional filters or globs for included files. Handy if you specify large directories but only want certain file types
    #[arg(short = 'f', long = "file-filter", value_name = "PATTERNS")]
    #[arg(help_heading = "Advanced Options")]
    file_filter: Option<String>,

    /// One or more files, directories, or globs to process
    #[arg(required = true, value_name = "PATHS")]
    #[arg(help_heading = "Arguments")]
    paths: Vec<PathBuf>,
}

/// Revert all or part of a previous replacement operation. Supports listing hunks, partial revert, and interactive hunk selection.
#[derive(Parser, Debug)]
#[command(about = "Undo or partially revert a previous replacement operation")]
#[command(long_about = "Revert all or part of a previous replacement operation. Supports listing hunks, partial revert, and interactive hunk selection.")]
#[command(after_help = "\
Examples:
  # Full revert
  rustscout-cli replace undo 1672834872

  # List hunks
  rustscout-cli replace undo 1672834872 --list-hunks

  # Partial revert
  rustscout-cli replace undo 1672834872 --hunks 0,1,3

  # Interactive
  rustscout-cli replace undo 1672834872 -i

  # Preview
  rustscout-cli replace undo 1672834872 --hunks 2,4 --preview")]
struct ReplaceUndo {
    /// ID of the replacement operation to revert
    #[arg(value_name = "ID")]
    #[arg(help_heading = "Arguments")]
    id: String,

    /// Lists all hunks (change chunks) in the specified operation
    #[arg(short = 'l', long = "list-hunks")]
    #[arg(help_heading = "Options")]
    list_hunks: bool,

    /// Revert only these hunk indices (comma-separated). If omitted, reverts all hunks
    #[arg(short = 'u', long = "hunks", value_name = "HUNKS")]
    #[arg(conflicts_with = "interactive")]
    #[arg(help_heading = "Options")]
    hunks: Option<String>,

    /// Shows the content that would be restored without changing files
    #[arg(short = 'p', long = "preview")]
    #[arg(help_heading = "Options")]
    preview: bool,

    /// Interactive "approve or skip" flow for each hunk, letting you partially revert
    #[arg(short = 'i', long = "interactive")]
    #[arg(conflicts_with = "hunks")]
    #[arg(help_heading = "Options")]
    interactive: bool,

    /// Skip all confirmations. Use with caution
    #[arg(short = 'f', long = "force", alias = "yes")]
    #[arg(help_heading = "Options")]
    force: bool,

    /// Override the default .rustscout/undo path where backup data is stored
    #[arg(long = "undo-dir", default_value = ".rustscout/undo")]
    #[arg(value_name = "UNDO_DIR")]
    #[arg(help_heading = "Options")]
    undo_dir: PathBuf,
}

/// Arguments for interactive search
#[derive(Parser, Debug)]
#[command(about = "Interactively search your codebase match by match. Navigate results using keyboard shortcuts, display context lines, skip files, and optionally edit matches on the spot")]
#[command(after_help = "\
Navigation:
  n or Right Arrow: next match
  p or Left Arrow: previous match
  f: skip the entire file
  a: skip all remaining matches
  q: quit
  e: edit the matched line(s)

Editing:
When you enter edit mode:
  - Use arrow keys to select a line
  - Press Enter to edit that line in place
  - Press r to replace the current match (or do a local fix)
  - Save or Cancel your edits

Example Usage:

1. Basic Interactive Search
   rustscout-cli interactive-search \\
     -p \"TODO\" \\
     --root ./src
   Steps through every \"TODO\" match in the src directory, showing 2 lines of context on each side.

2. Regex with 4 Lines of Context
   rustscout-cli interactive-search \\
     -p \"fn (\\w+)\\(\" -r true \\
     -B 4 -A 4 \\
     -d . \\
     -x rs
   Lets you navigate and optionally edit each Rust function definition, with 4 lines of context before/after.

3. Ignore Patterns, Multi-Extensions
   rustscout-cli interactive-search \\
     -p \"RefactorMe\" \\
     -x rs,go,js \\
     -g \"**/vendor/**\" \\
     -I
   Skips vendor/ directories, only searches .rs, .go, and .js files, using incremental caching for faster subsequent runs.

4. Large Repos with Parallel Threads
   rustscout-cli interactive-search \\
     -p \"FIXME\" \\
     -j 8
   Processes matches with 8 threads, ensuring a fast interactive experience even in huge projects.")]
struct InteractiveSearchArgs {
    /// Adds one or more search patterns (literal by default).
    /// Can be specified multiple times:
    ///   -p "MyClass" -p "TODO" ...
    /// Each pattern is evaluated or-style unless you combine with advanced config or TUI selections.
    #[arg(short = 'p', long = "pattern", help_heading = "Core Pattern Options")]
    patterns: Vec<String>,

    /// Legacy positional patterns (deprecated)
    #[arg(hide = true)]
    legacy_patterns: Vec<String>,

    /// Toggles regex interpretation for the most recently added pattern.
    /// Default: false (treat pattern as literal)
    /// Example:
    ///   -p "fn (\w+)\(" -r true
    /// Regex matching for Rust function definitions.
    #[arg(short = 'r', long = "regex", action = clap::ArgAction::Append, help_heading = "Core Pattern Options")]
    is_regex: Vec<bool>,

    /// Controls word boundary matching:
    /// - strict: Only match entire words
    /// - partial: Loose boundary handling
    /// - none (default): No boundary constraint
    /// Shorthand: -w, --word-boundary = --boundary-mode strict
    #[arg(short = 'b', long = "boundary-mode", default_value = "none", help_heading = "Core Pattern Options")]
    boundary_mode: String,

    /// Shorthand for --boundary-mode strict
    #[arg(short = 'w', long = "word-boundary", conflicts_with = "boundary_mode", help_heading = "Core Pattern Options")]
    word_boundary: bool,

    /// Defines how hyphens are treated in boundary detection (boundary or joining).
    /// Default: joining (hyphens considered part of a word).
    #[arg(short = 'y', long = "hyphen-mode", default_value = "joining", help_heading = "Core Pattern Options")]
    hyphen_mode: String,

    /// Specifies the root directory to search.
    /// Default: . (current directory)
    #[arg(short = 'd', long = "root", default_value = ".", help_heading = "File & Directory Options")]
    root: PathBuf,

    /// Comma-separated list of file extensions to include.
    /// Example: -x rs,py,md
    #[arg(short = 'x', long = "extensions", help_heading = "File & Directory Options")]
    extensions: Option<String>,

    /// Glob patterns to ignore certain files/folders.
    /// Example: --ignore "**/node_modules/**" to skip dependencies.
    #[arg(short = 'g', long = "ignore", help_heading = "File & Directory Options")]
    ignore: Vec<String>,

    /// Number of context lines before each match (default: 2)
    #[arg(short = 'B', long = "context-before", default_value = "2", help_heading = "Interactive Navigation & Context")]
    context_before: usize,

    /// Number of context lines after each match (default: 2)
    #[arg(short = 'A', long = "context-after", default_value = "2", help_heading = "Interactive Navigation & Context")]
    context_after: usize,

    /// Number of threads for parallel searching.
    /// Default: number of CPU cores
    #[arg(short = 'j', long = "threads", help_heading = "Performance & Caching")]
    threads: Option<NonZeroUsize>,

    /// Enables incremental caching of file checksums or Git data. Improves speed on repeated searches.
    #[arg(short = 'I', long = "incremental", help_heading = "Performance & Caching")]
    incremental: bool,

    /// Path to cache file (default: .rustscout-cache.json)
    #[arg(short = 'C', long = "cache-path", help_heading = "Performance & Caching")]
    cache_path: Option<PathBuf>,

    /// Method for detecting changed files: auto (default), git, or signature
    #[arg(short = 'S', long = "cache-strategy", default_value = "auto", help_heading = "Performance & Caching")]
    cache_strategy: String,

    /// Specifies how to handle invalid UTF-8:
    /// - failfast (default)
    /// - lossy (replace invalid sequences)
    #[arg(short = 'E', long = "encoding", default_value = "failfast", help_heading = "Misc. & Logging")]
    encoding: String,

    /// Disables colored output in the TUI. Suitable for terminals that lack color support.
    #[arg(short = 'N', long = "no-color", help_heading = "Misc. & Logging")]
    no_color: bool,
}

#[derive(Subcommand, Debug)]
#[command(about = "RustScout Workspace Management")]
#[command(long_about = "All short flags and documentation are designed to position RustScout as the market leader in code search and replace, with a user-friendly yet powerful workspace experience.")]
enum WorkspaceCommands {
    /// Initialize a new RustScout workspace
    Init(WorkspaceInit),

    /// Display metadata and status of the current workspace
    Info(WorkspaceInfo),
}

#[derive(Parser, Debug)]
#[command(about = "Creates a .rustscout folder in the specified directory (or current directory), saving workspace metadata in JSON or YAML")]
#[command(long_about = "This is the first step to enabling advanced code search & replace features (undo, caching, etc.).")]
#[command(after_help = "\
Example Workflows:

1. Basic Initialization
   rustscout-cli workspace init
   Initializes .rustscout/workspace.json in the current directory.

2. Specify a Different Directory
   rustscout-cli workspace init --dir ~/my_project
   Creates .rustscout under ~/my_project.

3. YAML Format
   rustscout-cli workspace init --format yaml
   Writes .rustscout/workspace.yaml.

4. Force Overwrite
   rustscout-cli workspace init -F
   Replaces any existing .rustscout folder and config.")]
struct WorkspaceInit {
    /// The directory in which to initialize the workspace.
    /// Default: current directory (.)
    #[arg(short = 'd', long = "dir", value_name = "DIR", help_heading = "Options")]
    dir: Option<PathBuf>,

    /// The metadata file format to use (json or yaml).
    /// Default: json
    /// If yaml is chosen, creates workspace.yaml; if json, creates workspace.json.
    #[arg(short = 'f', long = "format", default_value = "json", value_name = "FORMAT", help_heading = "Options")]
    format: String,

    /// Overwrite any existing .rustscout folder or config files without confirmation.
    /// Useful if you want to "repair" a partial or outdated workspace setup.
    /// Use with caution—this discards any old config in .rustscout.
    #[arg(short = 'F', long = "force", help_heading = "Options")]
    force: bool,
}

#[derive(Parser, Debug)]
#[command(about = "Displays metadata and status of the current (or specified) RustScout workspace")]
#[command(long_about = "Displays metadata and status of the current (or specified) RustScout workspace—helpful for verifying the root path, format, version, and other settings.")]
#[command(after_help = "\
Output / Behavior:
- Root Path: The canonical root of the workspace.
- Workspace Version: The RustScout workspace version (if stored in workspace.json / workspace.yaml).
- Format: json or yaml, whichever you used at init.
- Global Config: If any global config is stored in the workspace metadata (e.g., ignore patterns).
- Existence of .rustscout/undo or other workspace features.

Example:
  rustscout-cli workspace info
Outputs:
  Workspace Root: /home/user/my_project
  Version: 1.2.3
  Format: json
  Global Config: ignore_patterns=[...], ...")]
struct WorkspaceInfo {
    /// The directory whose workspace info you want to show.
    /// Default: current directory (.)
    /// If .rustscout isn't found, tries to walk upward to detect the workspace root.
    #[arg(short = 'd', long = "dir", value_name = "DIR", help_heading = "Options")]
    dir: Option<PathBuf>,
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
    setup_logging(&cli.verbosity)?;

    match cli.command {
        Commands::Search(args) => {
            handle_search(*args, &cli.verbosity)?;
        }
        Commands::Replace { command } => {
            handle_replace(command, &cli.verbosity)?;
        }
        Commands::InteractiveSearch(args) => {
            handle_interactive_search(*args, &cli.verbosity)?;
        }
        Commands::Workspace { command } => {
            handle_workspace(command)?;
        }
    }
    Ok(())
}

fn handle_search(args: CliSearchConfig, verbosity: &str) -> Result<()> {
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
        log_level: verbosity.to_string(),
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

fn handle_replace(command: ReplaceCommands, verbosity: &str) -> Result<()> {
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
                log_level: verbosity.to_string(),
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
        WorkspaceCommands::Info(args) => {
            let dir = args.dir.unwrap_or_else(|| PathBuf::from("."));
            let metadata = rustscout::workspace::WorkspaceMetadata::load(&dir)?;
            println!("Workspace Root: {}", metadata.root_path.display());
            println!("Version: {}", metadata.version);
            println!("Format: {}", metadata.format);
            if let Some(config) = metadata.global_config {
                println!("Global Config:");
                if !config.ignore_patterns.is_empty() {
                    println!("  Ignore Patterns: {:?}", config.ignore_patterns);
                }
                if let Some(exts) = config.default_extensions {
                    println!("  Default Extensions: {:?}", exts);
                }
            }
            Ok(())
        }
    }
}

fn handle_interactive_search(args: InteractiveSearchArgs, verbosity: &str) -> Result<()> {
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
    };

    // Convert args to search config with the global verbosity
    let config = rustscout::search::interactive_search::convert_args_to_config(&lib_args, verbosity)?;

    rustscout::search::interactive_search::run_interactive_search(&lib_args, &config)?;
    Ok(())
}
