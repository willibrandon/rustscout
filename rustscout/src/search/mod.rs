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
/// This module demonstrates several parallel processing patterns that are similar to .NET:
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
pub mod engine;
pub mod matcher;
pub mod processor;
pub mod interactive_search;

pub use engine::search;
pub use matcher::PatternMatcher;
pub use processor::FileProcessor;
