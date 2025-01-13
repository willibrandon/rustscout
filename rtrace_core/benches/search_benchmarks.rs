use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rtrace_core::{Config, search::search};
use std::path::PathBuf;
use tempfile::tempdir;
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;

fn create_test_files(dir: &tempfile::TempDir, file_count: usize, lines_per_file: usize) -> std::io::Result<()> {
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

fn bench_simple_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 10, 100).unwrap();

    let mut group = c.benchmark_group("Simple Pattern Search");
    group.sample_size(10);

    let config = Config {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

    let config = Config {
        pattern: String::from(r"FIXME:.*bug.*line \d+"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
    };

    group.bench_function("search_fixme_regex", |b| {
        b.iter(|| {
            search(black_box(&config)).unwrap();
        });
    });

    group.finish();
}

fn bench_file_scaling(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 50, 20).unwrap(); // More files, fewer lines each

    let mut group = c.benchmark_group("File Count Scaling");
    group.sample_size(10);

    let base_config = Config {
        pattern: String::from("TODO"),
        root_path: PathBuf::from(dir.path()),
        ignore_patterns: vec![],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
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

criterion_group!(benches, bench_simple_pattern, bench_regex_pattern, bench_file_scaling);
criterion_main!(benches);
