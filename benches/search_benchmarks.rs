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

fn create_large_test_file(dir: &tempfile::TempDir, size_mb: usize) -> std::io::Result<PathBuf> {
    let file_path = dir.path().join("large_test.txt");
    let mut file = File::create(&file_path)?;
    let lines_needed = (size_mb * 1024 * 1024) / 100; // Approximate lines needed for target size
    
    for i in 0..lines_needed {
        writeln!(
            file,
            "This is line {} with some searchable content: TODO implement feature XYZ\n\
             Here's another line {} with different content: nothing special\n\
             And a third line {} with a FIXME: need to optimize this",
            i, i, i
        )?;
    }
    
    Ok(file_path)
}

fn bench_simple_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100).unwrap();

    let mut group = c.benchmark_group("Simple Pattern Search");
    group.sample_size(10);

    let config = SearchConfig {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
    };

    group.bench_function("search_todo", |b| {
        b.iter(|| {
            search(black_box(&config)).unwrap();
        });
    });

    group.finish();
}

fn bench_regex_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100).unwrap();

    let mut group = c.benchmark_group("Regex Pattern Search");
    group.sample_size(10);

    let config = SearchConfig {
        pattern: String::from(r"FIXME:.*bug.*line \d+"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
    };

    group.bench_function("search_fixme_regex", |b| {
        b.iter(|| {
            search(black_box(&config)).unwrap();
        });
    });

    group.finish();
}

fn bench_repeated_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100).unwrap();

    let mut group = c.benchmark_group("Repeated Pattern Search");
    group.sample_size(10);

    let patterns = [
        r"TODO",
        r"FIXME:.*bug.*line \d+",
        r"TODO",  // Repeated simple pattern
        r"FIXME:.*bug.*line \d+",  // Repeated regex pattern
    ];

    for (i, pattern) in patterns.iter().enumerate() {
        let config = SearchConfig {
            pattern: pattern.to_string(),
            root_path: PathBuf::from(dir.path()),
            ignore_patterns: vec![],
            file_extensions: None,
            stats_only: false,
            thread_count: NonZeroUsize::new(1).unwrap(),
            log_level: "warn".to_string(),
        };

        group.bench_function(format!("search_pattern_{}", i), |b| {
            b.iter(|| {
                search(black_box(&config)).unwrap();
            });
        });
    }

    group.finish();
}

fn bench_file_scaling(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 50, 20).unwrap(); // More files, fewer lines each

    let mut group = c.benchmark_group("File Count Scaling");
    group.sample_size(10);

    let base_config = SearchConfig {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
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
}

fn bench_large_file(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let large_file = create_large_test_file(&dir, 20).unwrap(); // 20MB file

    let mut group = c.benchmark_group("Large File Processing");
    group.sample_size(10);

    let config = SearchConfig {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
    };

    group.bench_function("search_large_file", |b| {
        b.iter(|| {
            search(black_box(&config)).unwrap();
        });
    });

    // Test regex pattern on large file
    let regex_config = SearchConfig {
        pattern: String::from(r"FIXME:.*optimize.*\d+"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
    };

    group.bench_function("search_large_file_regex", |b| {
        b.iter(|| {
            search(black_box(&regex_config)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_pattern,
    bench_regex_pattern,
    bench_repeated_pattern,
    bench_file_scaling,
    bench_large_file
);
criterion_main!(benches); 