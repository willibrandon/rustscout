/// This module implements concurrent file searching functionality, demonstrating Rust's parallel processing
/// capabilities compared to .NET's Task Parallel Library (TPL).
///
/// # .NET vs Rust Parallel Processing
///
/// In .NET, you might implement parallel search using:
/// ```csharp
/// var results = files.AsParallel()
///     .Select(file => SearchFile(file))
///     .Where(result => result.Matches.Any())
///     .ToList();
/// ```
///
/// In Rust, we use Rayon's parallel iterators which provide similar functionality but with
/// guaranteed memory safety through Rust's ownership system:
/// ```rust,ignore
/// let results: Vec<_> = files.par_iter()
///     .map(|file| search_file(file))
///     .filter_map(|r| r.ok())
///     .filter(|r| !r.matches.is_empty())
///     .collect();
/// ```
///
/// # Performance Optimizations
///
/// This implementation includes several optimizations:
/// 1. **File Size Stratification**: Files are grouped by size for optimal processing
///    (similar to .NET's partitioning strategies in TPL)
/// 2. **Pattern-Based Strategy**: Simple patterns use fast literal search while complex
///    patterns use regex (similar to .NET's Regex compilation optimization)
/// 3. **Chunked Processing**: Large files are processed in chunks to balance thread workload
///    (similar to .NET's TPL chunking strategies)
///
/// # Error Handling
///
/// Unlike .NET's exception handling:
/// ```csharp
/// try {
///     var result = SearchFiles(pattern);
/// } catch (IOException ex) {
///     // Handle error
/// }
/// ```
///
/// Rust uses Result for error handling:
/// ```rust,ignore
/// match search(config) {
///     Ok(result) => // Process result,
///     Err(e) => // Handle error
/// }
/// ```
///
/// # Parallel Processing Patterns
///
/// This function demonstrates several parallel processing patterns that are similar to .NET:
///
/// 1. **Parallel File Processing**
///    .NET:
///    ```csharp
///    var results = files.AsParallel()
///        .Select(file => ProcessFile(file))
///        .ToList();
///    ```
///    Rust/Rayon:
///    ```rust,ignore
///    let results: Vec<_> = files.par_iter()
///        .map(|file| process_file(file))
///        .collect();
///    ```
///
/// 2. **Work Stealing Thread Pool**
///    .NET uses TPL's work-stealing pool:
///    ```csharp
///    var parallelOptions = new ParallelOptions { MaxDegreeOfParallelism = Environment.ProcessorCount };
///    Parallel.ForEach(files, parallelOptions, file => ProcessFile(file));
///    ```
///    Rust uses Rayon's work-stealing pool:
///    ```rust,ignore
///    files.par_iter().for_each(|file| process_file(file));
///    ```
///
/// # Memory Management
///
/// Unlike .NET where the GC handles memory:
/// ```csharp
/// using var reader = new StreamReader(path);
/// ```
///
/// In Rust, we explicitly manage buffers:
/// ```rust,ignore
/// let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
/// let mut line_buffer = String::with_capacity(256);
/// ```
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, info, trace, warn};

use crate::config::SearchConfig;
use crate::errors::{SearchError, SearchResult};
use crate::filters::should_include_file;
use crate::results::{FileResult, Match, SearchResult as SearchOutput};

// Thresholds for optimization strategies
const SMALL_FILE_THRESHOLD: u64 = 32 * 1024; // 32KB
const SIMPLE_PATTERN_THRESHOLD: usize = 32; // Pattern length for simple search
const MIN_CHUNK_SIZE: usize = 16; // Minimum files per chunk to reduce overhead
const MAX_CHUNK_SIZE: usize = 256; // Maximum files per chunk to ensure good load balancing
const BUFFER_CAPACITY: usize = 8192; // Initial buffer size for reading files

/// Determines if a pattern is "simple" enough for optimized literal search.
///
/// This is similar to .NET's Regex.IsMatch optimization where simple patterns
/// use string.Contains() instead of full regex matching.
///
/// # Arguments
///
/// * `pattern` - The search pattern to analyze
///
/// # Returns
///
/// `true` if the pattern can use simple string matching, `false` if it needs regex
fn is_simple_pattern(pattern: &str) -> bool {
    let is_simple = pattern.len() < SIMPLE_PATTERN_THRESHOLD
        && !pattern.contains(['*', '+', '?', '[', ']', '(', ')', '|', '^', '$', '.', '\\']);

    debug!(
        "Pattern '{}' is {}",
        pattern,
        if is_simple { "simple" } else { "complex" }
    );
    is_simple
}

/// Fast path for searching small files with simple patterns.
///
/// This is analogous to .NET's optimized string search in StringBuilder or
/// String.IndexOf() for simple patterns.
///
/// # Arguments
///
/// * `path` - Path to the file to search
/// * `pattern` - The literal pattern to search for
///
/// # Returns
///
/// A Result containing FileResult with any matches found
///
/// # Error Handling
///
/// Returns io::Error if file operations fail, similar to .NET's IOException
fn search_file_simple(path: &Path, pattern: &str) -> SearchResult<FileResult> {
    trace!("Simple search in file: {}", path.display());
    let content = fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
        std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
        _ => SearchError::IoError(e),
    })?;

    let mut matches = Vec::new();
    let mut line_number = 0;
    let mut last_match = 0;

    for line in content.lines() {
        line_number += 1;
        if let Some(index) = line.find(pattern) {
            trace!("Found match at line {}: {}", line_number, line);
            matches.push(Match {
                line_number,
                line_content: line.to_string(),
                start: index,
                end: index + pattern.len(),
            });
            last_match = matches.len();
        }
        if line_number > 100 && last_match == 0 {
            debug!("No matches in first 100 lines, skipping rest of file");
            break;
        }
    }

    debug!("Found {} matches in file {}", matches.len(), path.display());
    Ok(FileResult {
        path: path.to_path_buf(),
        matches,
    })
}

/// Regular path for searching files with regex patterns.
///
/// Similar to .NET's Regex.Matches() but with explicit buffer management
/// instead of relying on garbage collection.
///
/// # Arguments
///
/// * `path` - Path to the file to search
/// * `regex` - The compiled regex pattern
///
/// # Returns
///
/// A Result containing FileResult with any matches found
///
/// # Memory Management
///
/// Unlike .NET where the GC handles memory:
/// ```csharp
/// using var reader = new StreamReader(path);
/// ```
///
/// In Rust, we explicitly manage buffers:
/// ```rust,ignore
/// let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
/// let mut line_buffer = String::with_capacity(256);
/// ```
fn search_file_regex(path: &Path, regex: &Regex) -> SearchResult<FileResult> {
    trace!("Regex search in file: {}", path.display());
    let file = File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => SearchError::file_not_found(path),
        std::io::ErrorKind::PermissionDenied => SearchError::permission_denied(path),
        _ => SearchError::IoError(e),
    })?;

    let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, file);
    let mut matches = Vec::new();
    let mut line_buffer = String::with_capacity(256);
    let mut line_number = 0;
    let mut last_match = 0;

    while reader.read_line(&mut line_buffer)? > 0 {
        line_number += 1;
        if regex.is_match(&line_buffer) {
            trace!(
                "Found match at line {}: {}",
                line_number,
                line_buffer.trim()
            );
            for capture in regex.find_iter(&line_buffer) {
                matches.push(Match {
                    line_number,
                    line_content: line_buffer.clone(),
                    start: capture.start(),
                    end: capture.end(),
                });
                last_match = line_number;
            }
        }
        if line_number > 100 && last_match == 0 {
            debug!("No matches in first 100 lines, skipping rest of file");
            break;
        }
        line_buffer.clear();
    }

    debug!("Found {} matches in file {}", matches.len(), path.display());
    Ok(FileResult {
        path: path.to_path_buf(),
        matches,
    })
}

/// Performs a concurrent search across files in the specified directory.
///
/// # Arguments
///
/// * `config` - The search configuration containing pattern and options
///
/// # Returns
///
/// A Result containing SearchResult with all matches found
///
/// # Error Handling
///
/// Returns SearchError for file operations or invalid regex patterns
pub fn search(config: &SearchConfig) -> SearchResult<SearchOutput> {
    info!("Starting search with pattern: {}", config.pattern);

    if config.pattern.is_empty() {
        warn!("Empty search pattern provided");
        return Ok(SearchOutput::new());
    }

    let is_simple = is_simple_pattern(&config.pattern);
    let regex = if !is_simple {
        debug!("Compiling regex pattern: {}", config.pattern);
        Some(Regex::new(&config.pattern).map_err(|e| SearchError::invalid_pattern(e.to_string()))?)
    } else {
        None
    };

    let mut builder = WalkBuilder::new(&config.root_path);
    builder
        .hidden(true)
        .standard_filters(true)
        .require_git(false);

    for pattern in &config.ignore_patterns {
        debug!("Adding ignore pattern: {}", pattern);
        builder.add_ignore(pattern);
    }

    builder.add_custom_ignore_filename(".gitignore");
    builder.add_ignore("target");
    builder.add_ignore(".git");

    let walker = builder.build();

    let mut small_files = Vec::new();
    let mut large_files = Vec::new();

    debug!("Scanning directory: {}", config.root_path.display());
    for entry in walker
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| should_include_file(e.path(), &config.file_extensions, &config.ignore_patterns))
    {
        match entry.metadata() {
            Ok(metadata) if metadata.len() < SMALL_FILE_THRESHOLD => {
                trace!("Adding small file: {}", entry.path().display());
                small_files.push(entry)
            }
            _ => {
                trace!("Adding large file: {}", entry.path().display());
                large_files.push(entry)
            }
        }
    }

    info!(
        "Found {} files to search ({} small, {} large)",
        small_files.len() + large_files.len(),
        small_files.len(),
        large_files.len()
    );

    let mut final_result = SearchOutput::new();

    if !small_files.is_empty() {
        debug!("Processing {} small files", small_files.len());
        let results: Vec<_> = if is_simple {
            small_files
                .par_iter()
                .map(|entry| search_file_simple(entry.path(), &config.pattern))
                .filter_map(|r| r.ok())
                .filter(|r| !r.matches.is_empty())
                .collect()
        } else if let Some(ref regex) = regex {
            small_files
                .par_iter()
                .map(|entry| search_file_regex(entry.path(), regex))
                .filter_map(|r| r.ok())
                .filter(|r| !r.matches.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        debug!("Found matches in {} small files", results.len());
        results
            .into_iter()
            .for_each(|r| final_result.add_file_result(r));
    }

    if !large_files.is_empty() {
        debug!("Processing {} large files", large_files.len());
        let chunk_size = (large_files.len() / rayon::current_num_threads())
            .clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE);
        debug!("Using chunk size of {} for parallel processing", chunk_size);

        let results: Vec<_> = large_files
            .par_chunks(chunk_size)
            .flat_map(|chunk| {
                chunk
                    .par_iter()
                    .map(|entry| {
                        if let Some(ref regex) = regex {
                            search_file_regex(entry.path(), regex)
                        } else {
                            search_file_simple(entry.path(), &config.pattern)
                        }
                    })
                    .filter_map(|r| r.ok())
                    .filter(|r| !r.matches.is_empty())
                    .collect::<Vec<_>>()
            })
            .collect();

        debug!("Found matches in {} large files", results.len());
        results
            .into_iter()
            .for_each(|r| final_result.add_file_result(r));
    }

    info!(
        "Search completed: {} matches in {} files",
        final_result.total_matches, final_result.files_with_matches
    );

    Ok(final_result)
}
