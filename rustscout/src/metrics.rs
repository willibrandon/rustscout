use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

use crate::search::processor::{LARGE_FILE_THRESHOLD, SMALL_FILE_THRESHOLD};

/// Tracks memory usage and performance metrics
#[derive(Debug, Clone)]
pub struct MemoryMetrics {
    // Memory usage metrics
    total_allocated: Arc<AtomicU64>,
    peak_allocated: Arc<AtomicU64>,
    mmap_allocated: Arc<AtomicU64>,
    cache_size: Arc<AtomicU64>,

    // Cache metrics
    cache_hits: Arc<AtomicU64>,
    cache_misses: Arc<AtomicU64>,

    // File processing metrics
    small_files_processed: Arc<AtomicU64>,
    buffered_files_processed: Arc<AtomicU64>,
    mmap_files_processed: Arc<AtomicU64>,
}

impl MemoryMetrics {
    /// Creates a new MemoryMetrics instance
    pub fn new() -> Self {
        Self {
            total_allocated: Arc::new(AtomicU64::new(0)),
            peak_allocated: Arc::new(AtomicU64::new(0)),
            mmap_allocated: Arc::new(AtomicU64::new(0)),
            cache_size: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            small_files_processed: Arc::new(AtomicU64::new(0)),
            buffered_files_processed: Arc::new(AtomicU64::new(0)),
            mmap_files_processed: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Records memory allocation
    pub fn record_allocation(&self, bytes: u64) {
        let total = self.total_allocated.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let mut peak = self.peak_allocated.load(Ordering::Relaxed);
        while total > peak {
            match self.peak_allocated.compare_exchange_weak(
                peak,
                total,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }
        debug!("Memory allocated: {} bytes, total: {} bytes", bytes, total);
    }

    /// Records memory deallocation
    pub fn record_deallocation(&self, bytes: u64) {
        let total = self.total_allocated.fetch_sub(bytes, Ordering::Relaxed) - bytes;
        debug!(
            "Memory deallocated: {} bytes, total: {} bytes",
            bytes, total
        );
    }

    /// Records memory mapped file
    pub fn record_mmap(&self, bytes: u64) {
        let total = self.mmap_allocated.fetch_add(bytes, Ordering::Relaxed) + bytes;
        debug!(
            "Memory mapped: {} bytes, total mapped: {} bytes",
            bytes, total
        );
    }

    /// Records unmapping of file
    pub fn record_munmap(&self, bytes: u64) {
        let total = self.mmap_allocated.fetch_sub(bytes, Ordering::Relaxed) - bytes;
        debug!(
            "Memory unmapped: {} bytes, total mapped: {} bytes",
            bytes, total
        );
    }

    /// Records cache operation
    pub fn record_cache_operation(&self, size_delta: i64, hit: bool) {
        if size_delta > 0 {
            self.cache_size
                .fetch_add(size_delta as u64, Ordering::Relaxed);
        } else {
            self.cache_size
                .fetch_sub((-size_delta) as u64, Ordering::Relaxed);
        }

        if hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records file processing type
    pub fn record_file_processing(&self, size: u64) {
        if size < SMALL_FILE_THRESHOLD {
            self.small_files_processed.fetch_add(1, Ordering::Relaxed);
        } else if size >= LARGE_FILE_THRESHOLD {
            self.mmap_files_processed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.buffered_files_processed
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Gets current memory usage statistics
    pub fn get_stats(&self) -> MemoryStats {
        MemoryStats {
            total_allocated: self.total_allocated.load(Ordering::Relaxed),
            peak_allocated: self.peak_allocated.load(Ordering::Relaxed),
            mmap_allocated: self.mmap_allocated.load(Ordering::Relaxed),
            cache_size: self.cache_size.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            small_files: self.small_files_processed.load(Ordering::Relaxed),
            buffered_files: self.buffered_files_processed.load(Ordering::Relaxed),
            mmap_files: self.mmap_files_processed.load(Ordering::Relaxed),
        }
    }

    /// Logs current memory usage statistics
    pub fn log_stats(&self) {
        let stats = self.get_stats();
        info!(
            "Memory usage stats:\n\
             Total allocated: {} bytes\n\
             Peak allocated: {} bytes\n\
             Memory mapped: {} bytes\n\
             Cache size: {} bytes\n\
             Cache hits/misses: {}/{}\n\
             Files processed (small/buffered/mmap): {}/{}/{}",
            stats.total_allocated,
            stats.peak_allocated,
            stats.mmap_allocated,
            stats.cache_size,
            stats.cache_hits,
            stats.cache_misses,
            stats.small_files,
            stats.buffered_files,
            stats.mmap_files
        );
    }
}

impl Default for MemoryMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about memory usage and performance
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    pub total_allocated: u64,
    pub peak_allocated: u64,
    pub mmap_allocated: u64,
    pub cache_size: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub small_files: u64,
    pub buffered_files: u64,
    pub mmap_files: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_allocation_tracking() {
        let metrics = MemoryMetrics::new();

        // Test allocation
        metrics.record_allocation(1000);
        metrics.record_allocation(500);
        let stats = metrics.get_stats();
        assert_eq!(stats.total_allocated, 1500);
        assert_eq!(stats.peak_allocated, 1500);

        // Test deallocation
        metrics.record_deallocation(500);
        let stats = metrics.get_stats();
        assert_eq!(stats.total_allocated, 1000);
        assert_eq!(stats.peak_allocated, 1500); // Peak should remain unchanged
    }

    #[test]
    fn test_mmap_tracking() {
        let metrics = MemoryMetrics::new();

        metrics.record_mmap(5000);
        metrics.record_mmap(3000);
        let stats = metrics.get_stats();
        assert_eq!(stats.mmap_allocated, 8000);

        metrics.record_munmap(3000);
        let stats = metrics.get_stats();
        assert_eq!(stats.mmap_allocated, 5000);
    }

    #[test]
    fn test_cache_metrics() {
        let metrics = MemoryMetrics::new();

        // Test cache hit
        metrics.record_cache_operation(100, true);
        let stats = metrics.get_stats();
        assert_eq!(stats.cache_size, 100);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 0);

        // Test cache miss
        metrics.record_cache_operation(50, false);
        let stats = metrics.get_stats();
        assert_eq!(stats.cache_size, 150);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }

    #[test]
    fn test_file_processing_tracking() {
        let metrics = MemoryMetrics::new();

        metrics.record_file_processing(1000); // Small file
        metrics.record_file_processing(100000); // Buffered file
        metrics.record_file_processing(20_000_000); // Memory mapped file

        let stats = metrics.get_stats();
        assert_eq!(stats.small_files, 1);
        assert_eq!(stats.buffered_files, 1);
        assert_eq!(stats.mmap_files, 1);
    }
}
