#![allow(unused_must_use)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustscout::{
    cache::{ChangeDetectionStrategy, IncrementalCache},
    config::EncodingMode,
    search, SearchConfig,
};
use std::{fs::File, io::Write, num::NonZeroUsize};
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
            writeln!(
                file,
                "Line {} TODO: fix bug {} FIXME: optimize line {} NOTE: important task {}",
                j, j, j, j
            )?;
        }
    }
    Ok(())
}

fn create_base_config(dir: &tempfile::TempDir) -> SearchConfig {
    SearchConfig {
        pattern_definitions: vec![],
        root_path: dir.path().to_path_buf(),
        ignore_patterns: vec![".git/**".to_string()],
        file_extensions: None,
        stats_only: false,
        thread_count: NonZeroUsize::new(1).unwrap(),
        log_level: "warn".to_string(),
        context_before: 0,
        context_after: 0,
        incremental: false,
        cache_path: None,
        cache_strategy: ChangeDetectionStrategy::Auto,
        max_cache_size: None,
        use_compression: false,
        encoding_mode: EncodingMode::FailFast,
    }
}

fn bench_repeated_pattern(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 1, 10)?;

    let patterns = vec![
        "TODO",
        r"TODO:.*\d+",
        r"FIXME:.*bug.*line \d+",
        r"NOTE:.*important.*\d+",
    ];

    let mut group = c.benchmark_group("Repeated Pattern");
    group.sample_size(20);
    group.warm_up_time(std::time::Duration::from_secs(1));

    for (i, pattern) in patterns.iter().enumerate() {
        let mut config = create_base_config(&dir);
        config.pattern_definitions = vec![PatternDefinition {
            text: pattern.to_string(),
            is_regex: false,
            boundary_mode: WordBoundaryMode::None,
            hyphen_handling: HyphenHandling::default(),
        }];

        group.bench_function(format!("pattern_{}", i), |b| {
            b.iter_with_setup(
                || config.clone(),
                |cfg| {
                    black_box(search(&cfg).unwrap());
                },
            );
        });
    }

    group.finish();
    Ok(())
}

fn bench_file_scaling(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    let file_counts = vec![1, 10, 100, 1000];
    let base_config = create_base_config(&dir);

    let mut group = c.benchmark_group("File Scaling");
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_secs(1));

    for &count in &file_counts {
        // Clean up previous files
        if count > 1 {
            for i in 0..count - 1 {
                let _ = std::fs::remove_file(dir.path().join(format!("test_{}.txt", i)));
            }
        }

        // Create new test files
        create_test_files(&dir, count, 10)?;

        group.bench_function(format!("files_{}", count), |b| {
            b.iter_with_setup(
                || base_config.clone(),
                |config| {
                    black_box(search(&config).unwrap());
                },
            );
        });
    }

    group.finish();
    Ok(())
}

fn bench_incremental_search(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir()?;
    create_test_files(&dir, 20, 50)?;
    let cache_path = dir.path().join("cache.json");

    let mut base_config = create_base_config(&dir);
    base_config.incremental = true;
    base_config.cache_path = Some(cache_path.clone());

    let mut group = c.benchmark_group("Incremental Search");
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_secs(1));

    // Initial search (no cache)
    group.bench_function("initial_search", |b| {
        b.iter_with_setup(
            || {
                if cache_path.exists() {
                    let _ = std::fs::remove_file(&cache_path);
                }
                base_config.clone()
            },
            |config| {
                black_box(search(&config).unwrap());
            },
        );
    });

    // Subsequent search (with cache, no changes)
    group.bench_function("cached_search", |b| {
        b.iter_with_setup(
            || {
                if !cache_path.exists() {
                    search(&base_config).unwrap();
                }
                base_config.clone()
            },
            |config| {
                search(black_box(&config)).unwrap();
            },
        );
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
                search(black_box(&config)).unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();

    // Cleanup
    if cache_path.exists() {
        let _ = std::fs::remove_file(&cache_path);
    }
    Ok(())
}

fn bench_cache_operations(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 100, 20)?; // More files for cache benchmarks
    let cache_path = dir.path().join("cache.json");

    let mut base_config = create_base_config(&dir);
    base_config.incremental = true;
    base_config.cache_path = Some(cache_path.clone());

    let mut group = c.benchmark_group("Cache Operations");

    // Ensure clean state before each benchmark
    group.sample_size(10); // Reduce sample size to minimize race conditions
    group.warm_up_time(std::time::Duration::from_secs(1));

    // Cache creation - ensure clean state
    group.bench_function("cache_creation", |b| {
        b.iter_with_setup(
            || {
                if cache_path.exists() {
                    let _ = std::fs::remove_file(&cache_path);
                }
                base_config.clone()
            },
            |config| {
                search(black_box(&config)).unwrap();
            },
        );
    });

    // Cache loading - ensure cache exists
    group.bench_function("cache_loading", |b| {
        b.iter_with_setup(
            || {
                if !cache_path.exists() {
                    search(&base_config).unwrap();
                }
                &cache_path
            },
            |path| {
                let cache = IncrementalCache::load_from(black_box(path)).unwrap();
                black_box(cache);
            },
        );
    });

    // Cache with compression - clean state each time
    group.bench_function("compressed_cache", |b| {
        b.iter_with_setup(
            || {
                if cache_path.exists() {
                    let _ = std::fs::remove_file(&cache_path);
                }
                let mut config = base_config.clone();
                config.use_compression = true;
                config
            },
            |config| {
                search(black_box(&config)).unwrap();
            },
        );
    });

    group.finish();

    // Cleanup
    if cache_path.exists() {
        let _ = std::fs::remove_file(&cache_path);
    }
    Ok(())
}

fn bench_change_detection(c: &mut Criterion) -> std::io::Result<()> {
    let dir = tempdir().unwrap();
    create_test_files(&dir, 50, 20)?;

    let mut base_config = create_base_config(&dir);
    base_config.incremental = true;
    base_config.cache_path = Some(dir.path().join("cache.json"));

    // Check if git is available
    let git_available = std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    // Initialize git repo for git strategy testing
    let git_initialized = if git_available {
        // Configure git for CI environment
        let git_config = [
            ("user.name", "Benchmark Test"),
            ("user.email", "test@example.com"),
            ("init.defaultBranch", "main"),
            ("core.autocrlf", "false"), // Prevent CRLF conversion
        ];

        let mut success = true;
        for (key, value) in git_config.iter() {
            success &= std::process::Command::new("git")
                .args(["config", "--local", key, value])
                .current_dir(dir.path())
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
        }

        // Create .gitignore to exclude binary and temp files
        std::fs::write(
            dir.path().join(".gitignore"),
            "*.bin\n*.tmp\n*.idx\n*.pack\n",
        )?;

        success &= std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        // Only add text files to git
        success &= std::process::Command::new("git")
            .args(["add", "*.txt"])
            .current_dir(dir.path())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        success &= std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(dir.path())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        success
    } else {
        false
    };

    let mut group = c.benchmark_group("Change Detection");
    group.sample_size(10); // Reduce sample size
    group.warm_up_time(std::time::Duration::from_secs(1));

    // FileSignature strategy - always run
    group.bench_function("filesig_detection", |b| {
        b.iter_with_setup(
            || {
                let mut config = base_config.clone();
                config.cache_strategy = ChangeDetectionStrategy::FileSignature;
                config
            },
            |config| {
                black_box(search(&config).unwrap());
            },
        );
    });

    // Git strategy - only run if git is available and initialized
    if git_initialized {
        group.bench_function("git_detection", |b| {
            b.iter_with_setup(
                || {
                    let mut config = base_config.clone();
                    config.cache_strategy = ChangeDetectionStrategy::GitStatus;
                    config
                },
                |config| {
                    black_box(search(&config).unwrap());
                },
            );
        });
    }

    // Auto strategy - always run
    group.bench_function("auto_detection", |b| {
        b.iter_with_setup(
            || {
                let mut config = base_config.clone();
                config.cache_strategy = ChangeDetectionStrategy::Auto;
                config
            },
            |config| {
                black_box(search(&config).unwrap());
            },
        );
    });

    group.finish();
    Ok(())
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = bench_repeated_pattern, bench_file_scaling,
              bench_incremental_search, bench_cache_operations,
              bench_change_detection
}

#[test]
fn ensure_benchmarks_valid() {
    benches();
}

criterion_main!(benches);
