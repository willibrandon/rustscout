use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustscout::{
    cache::{ChangeDetectionStrategy, IncrementalCache},
    replace::{ReplacementConfig, ReplacementPlan, ReplacementTask},
    search, SearchConfig,
};
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tempfile::tempdir;

fn create_test_files(dir: &tempdir::TempDir, count: usize, lines_per_file: usize) {
    for i in 0..count {
        let file_path = dir.path().join(format!("test_{}.txt", i));
        let mut file = File::create(file_path).unwrap();
        for j in 0..lines_per_file {
            writeln!(
                file,
                "Line {} TODO: fix bug {} FIXME: optimize line {} NOTE: important task {}",
                j, j, j, j
            )
            .unwrap();
        }
    }
}

fn create_base_config(dir: &tempdir::TempDir) -> SearchConfig {
    SearchConfig {
        pattern: "TODO".to_string(),
        patterns: vec!["TODO".to_string()],
        root_path: dir.path().to_path_buf(),
        file_extensions: None,
        ignore_patterns: vec![],
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::FileSignature,
        max_cache_size: None,
        use_compression: false,
    }
}

fn create_base_replacement_config(dir: &tempdir::TempDir) -> ReplacementConfig {
    let undo_dir = dir.path().join("undo");
    ReplacementConfig {
        pattern: "TODO".to_string(),
        replacement: "DONE".to_string(),
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![],
        file_extensions: None,
        backup_enabled: true,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        capture_groups: vec![],
        undo_dir,
        is_regex: false,
        dry_run: false,
    }
}

fn bench_simple_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 1, 10);
    let config = create_base_config(&dir);

    let mut group = c.benchmark_group("Simple Pattern");
    group.bench_function("search", |b| {
        b.iter(|| black_box(search(&config).unwrap()));
    });
    group.finish();
}

fn bench_regex_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 1, 10);
    let mut config = create_base_config(&dir);
    config.pattern = r"TODO:.*\d+".to_string();
    config.patterns = vec![r"TODO:.*\d+".to_string()];

    let mut group = c.benchmark_group("Regex Pattern");
    group.bench_function("search", |b| {
        b.iter(|| black_box(search(&config).unwrap()));
    });
    group.finish();
}

fn bench_repeated_pattern(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 1, 10);

    let patterns = vec![
        "TODO",
        r"TODO:.*\d+",
        r"FIXME:.*bug.*line \d+",
        r"NOTE:.*important.*\d+",
    ];

    let mut group = c.benchmark_group("Repeated Pattern");
    for (i, pattern) in patterns.iter().enumerate() {
        let mut config = create_base_config(&dir);
        config.pattern = pattern.to_string();
        config.patterns = vec![pattern.to_string()];

        group.bench_function(format!("pattern_{}", i), |b| {
            b.iter(|| black_box(search(&config).unwrap()));
        });
    }
    group.finish();
}

fn bench_file_scaling(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let file_counts = vec![1, 10, 100, 1000];
    let base_config = create_base_config(&dir);

    let mut group = c.benchmark_group("File Scaling");
    for &count in &file_counts {
        create_test_files(&dir, count, 10);

        group.bench_function(format!("files_{}", count), |b| {
            b.iter(|| black_box(search(&base_config).unwrap()));
        });
    }
    group.finish();
}

fn bench_large_file(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("large.txt");
    let mut file = File::create(&file_path).unwrap();

    // Create a large file with 100K lines
    for i in 0..100_000 {
        writeln!(file, "Line {} TODO: fix this", i).unwrap();
    }

    let config = create_base_config(&dir);

    let mut group = c.benchmark_group("Large File");
    group.bench_function("search", |b| {
        b.iter(|| black_box(search(&config).unwrap()));
    });
    group.finish();
}

fn bench_simple_replacement(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 1, 10);
    let config = create_base_replacement_config(&dir);

    let mut group = c.benchmark_group("Simple Replacement");
    group.bench_function("replace", |b| {
        b.iter(|| {
            let mut plan = ReplacementPlan::new();
            plan.add_replacement(ReplacementTask {
                original_text: "TODO".to_string(),
                replacement_text: "DONE".to_string(),
                original_range: 0..4,
                config: config.clone(),
            });
            black_box(plan);
        });
    });
    group.finish();
}

fn bench_incremental_search(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 20, 50);
    let cache_path = dir.path().join("cache.json");

    let mut base_config = create_base_config(&dir);
    base_config.incremental = true;
    base_config.cache_path = Some(cache_path.clone());

    let mut group = c.benchmark_group("Incremental Search");
    
    // Initial search (no cache)
    group.bench_function("initial_search", |b| {
        b.iter(|| {
            let config = base_config.clone();
            black_box(search(&config).unwrap());
        });
    });

    // Subsequent search (with cache, no changes)
    group.bench_function("cached_search", |b| {
        b.iter(|| {
            let config = base_config.clone();
            black_box(search(&config).unwrap());
        });
    });

    // Search with some changes
    group.bench_function("search_with_changes", |b| {
        b.iter_batched(
            || {
                // Setup: Modify 20% of files
                for i in 0..4 {
                    let file_path = dir.path().join(format!("test_{}.txt", i));
                    let mut content = std::fs::read_to_string(&file_path).unwrap();
                    content.push_str("\nNew TODO item added\n");
                    std::fs::write(&file_path, content).unwrap();
                }
                base_config.clone()
            },
            |config| {
                black_box(search(&config).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_cache_operations(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 100, 20); // More files for cache benchmarks
    let cache_path = dir.path().join("cache.json");

    let mut base_config = create_base_config(&dir);
    base_config.incremental = true;
    base_config.cache_path = Some(cache_path.clone());

    let mut group = c.benchmark_group("Cache Operations");

    // Cache creation
    group.bench_function("cache_creation", |b| {
        b.iter(|| {
            let mut config = base_config.clone();
            if cache_path.exists() {
                std::fs::remove_file(&cache_path).unwrap();
            }
            black_box(search(&config).unwrap());
        });
    });

    // Cache loading
    group.bench_function("cache_loading", |b| {
        b.iter(|| {
            let cache = IncrementalCache::load_from(black_box(&cache_path)).unwrap();
            black_box(cache);
        });
    });

    // Cache with compression
    group.bench_function("compressed_cache", |b| {
        b.iter(|| {
            let mut config = base_config.clone();
            config.use_compression = true;
            black_box(search(&config).unwrap());
        });
    });

    group.finish();
}

fn bench_change_detection(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 50, 20);
    
    // Initialize git repo for git strategy testing
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "Initial commit"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let mut base_config = create_base_config(&dir);
    base_config.incremental = true;
    base_config.cache_path = Some(dir.path().join("cache.json"));

    let mut group = c.benchmark_group("Change Detection");

    // FileSignature strategy
    group.bench_function("filesig_detection", |b| {
        b.iter(|| {
            let mut config = base_config.clone();
            config.cache_strategy = ChangeDetectionStrategy::FileSignature;
            black_box(search(&config).unwrap());
        });
    });

    // Git strategy
    group.bench_function("git_detection", |b| {
        b.iter(|| {
            let mut config = base_config.clone();
            config.cache_strategy = ChangeDetectionStrategy::GitStatus;
            black_box(search(&config).unwrap());
        });
    });

    // Auto strategy
    group.bench_function("auto_detection", |b| {
        b.iter(|| {
            let mut config = base_config.clone();
            config.cache_strategy = ChangeDetectionStrategy::Auto;
            black_box(search(&config).unwrap());
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
    bench_large_file,
    bench_simple_replacement,
    bench_incremental_search,
    bench_cache_operations,
    bench_change_detection
);
criterion_main!(benches); 