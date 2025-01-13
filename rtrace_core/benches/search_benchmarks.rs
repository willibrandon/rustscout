use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rtrace_core::{search, Config};
use std::fs::{self, create_dir_all};
use std::path::Path;
use tempfile::TempDir;

// Helper function to create a test file with content
fn create_test_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

// Helper function to create a test project with specified size
fn create_test_project(dir: &Path, files: usize, lines_per_file: usize, matches_per_file: usize) {
    for i in 0..files {
        let mut content = String::with_capacity(lines_per_file * 50);
        for j in 0..lines_per_file {
            if j % (lines_per_file / matches_per_file) == 0 {
                content.push_str(&format!("Line {} with TODO: Fix this\n", j));
            } else {
                content.push_str(&format!("Line {} with some content\n", j));
            }
        }
        create_test_file(dir, &format!("src/file{}.rs", i), &content);
    }
}

fn bench_search_varying_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_varying_files");
    group.sample_size(10); // Reduce sample size for large benchmarks

    for files in [10, 50, 100].iter() {
        let temp_dir = TempDir::new().unwrap();
        create_test_project(&temp_dir.path(), *files, 100, 5);

        let config = Config::new("TODO: Fix this".to_string(), temp_dir.path().to_path_buf())
            .with_file_extensions(vec!["rs".to_string()]);

        group.bench_with_input(BenchmarkId::from_parameter(files), files, |b, _| {
            b.iter(|| {
                black_box(search::search(&config).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_search_varying_file_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_varying_file_sizes");
    group.sample_size(10);

    for lines in [100, 1000, 10000].iter() {
        let temp_dir = TempDir::new().unwrap();
        create_test_project(&temp_dir.path(), 1, *lines, lines / 20);

        let config = Config::new("TODO: Fix this".to_string(), temp_dir.path().to_path_buf())
            .with_file_extensions(vec!["rs".to_string()]);

        group.bench_with_input(BenchmarkId::from_parameter(lines), lines, |b, _| {
            b.iter(|| {
                black_box(search::search(&config).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_search_varying_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_varying_patterns");
    let temp_dir = TempDir::new().unwrap();
    create_test_project(&temp_dir.path(), 10, 1000, 50);

    let patterns = [
        ("simple", "TODO"),
        ("word_boundary", r"\bTODO\b"),
        ("complex", r"TODO:?\s*[A-Z][a-z]+(\s+[a-z]+)*"),
        ("with_colon", r"TODO:"),
        ("with_comment", r"//\s*TODO"),
    ];

    for (name, pattern) in patterns.iter() {
        let config = Config::new(pattern.to_string(), temp_dir.path().to_path_buf())
            .with_file_extensions(vec!["rs".to_string()]);

        group.bench_with_input(BenchmarkId::from_parameter(name), name, |b, _| {
            b.iter(|| {
                black_box(search::search(&config).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_search_with_threads(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_with_threads");
    let temp_dir = TempDir::new().unwrap();
    create_test_project(&temp_dir.path(), 100, 1000, 50);

    for threads in [1, 2, 4, 8].iter() {
        let config = Config::new("TODO: Fix this".to_string(), temp_dir.path().to_path_buf())
            .with_file_extensions(vec!["rs".to_string()])
            .with_thread_count(std::num::NonZeroUsize::new(*threads).unwrap());

        group.bench_with_input(BenchmarkId::from_parameter(threads), threads, |b, _| {
            b.iter(|| {
                black_box(search::search(&config).unwrap());
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_search_varying_files,
    bench_search_varying_file_sizes,
    bench_search_varying_patterns,
    bench_search_with_threads
);
criterion_main!(benches);
