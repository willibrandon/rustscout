use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustscout::{search, SearchConfig};
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tempfile::tempdir;

fn create_test_files(
    dir: &tempfile::TempDir,
    file_count: usize,
    lines_per_file: usize,
) -> std::io::Result<()> {
    for i in 0..file_count {
        let file_path = dir.path().join(format!("test_{}.txt", i));
        let mut file = File::create(file_path)?;
        for j in 0..lines_per_file {
            writeln!(file, "Line {} in file {}: TODO implement this", j, i)?;
            writeln!(file, "Another line {} in file {}: nothing special", j, i)?;
            writeln!(file, "FIXME: This is a bug in file {} line {}", i, j)?;
        }
    }
    Ok(())
}

fn bench_simple_pattern(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100)?;

    let mut group = c.benchmark_group("Simple Pattern Search");
    group.sample_size(10);

    let config = SearchConfig {
        patterns: vec!["TODO".to_string()],
        pattern: String::new(),
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
    };

    group.bench_function("search_todo", |b| {
        b.iter(|| {
            search(black_box(&config)).unwrap();
        });
    });

    group.finish();
    Ok(())
}

fn bench_regex_pattern(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100)?;

    let mut group = c.benchmark_group("Regex Pattern Search");
    group.sample_size(10);

    let config = SearchConfig {
        patterns: vec![r"FIXME:.*bug.*line \d+".to_string()],
        pattern: String::new(),
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
    };

    group.bench_function("search_fixme_regex", |b| {
        b.iter(|| {
            search(black_box(&config)).unwrap();
        });
    });

    group.finish();
    Ok(())
}

fn bench_repeated_pattern(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100)?;

    let mut group = c.benchmark_group("Repeated Pattern Search");
    group.sample_size(10);

    let patterns = [
        r"TODO",
        r"FIXME:.*bug.*line \d+",
        r"TODO",                  // Repeated simple pattern
        r"FIXME:.*bug.*line \d+", // Repeated regex pattern
    ];

    for (i, pattern) in patterns.iter().enumerate() {
        let config = SearchConfig {
            patterns: vec![pattern.to_string()],
            pattern: String::new(),
            root_path: dir.path().to_path_buf(),
            ignore_patterns: vec![],
            file_extensions: None,
            stats_only: false,
            thread_count: NonZeroUsize::new(1).unwrap(),
            log_level: "warn".to_string(),
            context_before: 0,
            context_after: 0,
        };

        group.bench_function(format!("search_pattern_{}", i), |b| {
            b.iter(|| {
                search(black_box(&config)).unwrap();
            });
        });
    }

    group.finish();
    Ok(())
}

fn bench_file_scaling(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 50, 20)?;

    let mut group = c.benchmark_group("File Count Scaling");
    group.sample_size(10);

    let base_config = SearchConfig {
        patterns: vec!["TODO".to_string()],
        pattern: String::new(),
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
    };

    // Test with different subsets of files
    for &file_count in &[5, 10, 25, 50] {
        group.bench_function(format!("files_{}", file_count), |b| {
            b.iter(|| {
                let mut config = base_config.clone();
                // Limit search to first n files
                config.ignore_patterns = (file_count..50)
                    .map(|i| format!("test_{}.txt", i))
                    .collect();
                search(black_box(&config)).unwrap();
            });
        });
    }

    group.finish();
    Ok(())
}

fn create_large_test_file(dir: &tempfile::TempDir, size_mb: usize) -> std::io::Result<PathBuf> {
    let file_path = dir.path().join("large_test.txt");
    let mut file = File::create(&file_path)?;

    // Create a line with a known pattern
    let line = "This is a test line with pattern_123 and another pattern_456\n";
    let lines_needed = (size_mb * 1024 * 1024) / line.len();

    for _ in 0..lines_needed {
        file.write_all(line.as_bytes())?;
    }

    Ok(file_path)
}

fn bench_large_file_search(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();

    // Create test files of different sizes
    let sizes = [10, 50, 100]; // File sizes in MB

    for &size in &sizes {
        let file_path = create_large_test_file(&dir, size)?;

        let mut group = c.benchmark_group(format!("large_file_{}mb", size));

        // Benchmark with different thread counts
        for threads in [1, 2, 4, 8].iter() {
            group.bench_with_input(format!("threads_{}", threads), threads, |b, &threads| {
                b.iter(|| {
                    let config = SearchConfig {
                        patterns: vec!["pattern_\\d+".to_string()],
                        pattern: String::new(),
                        root_path: file_path.parent().unwrap().to_path_buf(),
                        ignore_patterns: vec![],
                        file_extensions: None,
                        stats_only: false,
                        thread_count: NonZeroUsize::new(threads).unwrap(),
                        log_level: "warn".to_string(),
                        context_before: 0,
                        context_after: 0,
                    };
                    search(&config).unwrap()
                })
            });
        }

        group.finish();
    }

    Ok(())
}

fn bench_multiple_patterns(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100)?;

    let mut group = c.benchmark_group("Multiple Pattern Search");
    group.sample_size(10);

    // Test with multiple simple patterns
    let simple_config = SearchConfig {
        patterns: vec!["TODO".to_string(), "FIXME".to_string()],
        pattern: String::new(),
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
    };

    group.bench_function("search_multiple_simple", |b| {
        b.iter(|| {
            search(black_box(&simple_config)).unwrap();
        });
    });

    // Test with mixed simple and regex patterns
    let mixed_config = SearchConfig {
        patterns: vec!["TODO".to_string(), r"FIXME:.*bug.*line \d+".to_string()],
        pattern: String::new(),
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
    };

    group.bench_function("search_multiple_mixed", |b| {
        b.iter(|| {
            search(black_box(&mixed_config)).unwrap();
        });
    });

    group.finish();
    Ok(())
}

fn bench_context_lines(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();

    // Create test files of different sizes
    let sizes = [10]; // Only use a small file for context line benchmarks

    for &size in &sizes {
        let file_path = create_large_test_file(&dir, size)?;

        let mut group = c.benchmark_group("context_lines");

        // Test different context configurations
        let configs = [
            (0, 0), // No context
            (2, 0), // Before only
            (0, 2), // After only
            (2, 2), // Both before and after
            (5, 5), // Larger context
        ];

        for (before, after) in configs.iter() {
            group.bench_function(format!("context_b{}_a{}", before, after), |b| {
                b.iter(|| {
                    let config = SearchConfig {
                        patterns: vec!["pattern_\\d+".to_string()],
                        pattern: String::new(),
                        root_path: file_path.parent().unwrap().to_path_buf(),
                        ignore_patterns: vec![],
                        file_extensions: None,
                        stats_only: false,
                        thread_count: NonZeroUsize::new(1).unwrap(),
                        log_level: "warn".to_string(),
                        context_before: *before,
                        context_after: *after,
                    };
                    search(&config).unwrap()
                })
            });
        }

        group.finish();
    }

    Ok(())
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = bench_simple_pattern, bench_regex_pattern, bench_repeated_pattern,
              bench_file_scaling, bench_large_file_search, bench_context_lines,
              bench_multiple_patterns
);
criterion_main!(benches);
