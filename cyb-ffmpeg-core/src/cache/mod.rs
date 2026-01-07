//! Multi-tier frame cache module
//!
//! Provides L1/L2/L3 caching for fast frame access during scrubbing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::decoder::VideoFrame;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// L1 (hot) cache capacity
    pub l1_capacity: usize,

    /// L2 (keyframe) cache capacity
    pub l2_capacity: usize,

    /// L3 (cold) cache capacity
    pub l3_capacity: usize,

    /// Enable prefetch
    pub enable_prefetch: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            l1_capacity: 30,
            l2_capacity: 100,
            l3_capacity: 500,
            enable_prefetch: true,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStatistics {
    /// L1 entry count
    pub l1_entries: usize,

    /// L2 entry count
    pub l2_entries: usize,

    /// L3 entry count
    pub l3_entries: usize,

    /// L1 hit count
    pub l1_hit_count: u64,

    /// L2 hit count
    pub l2_hit_count: u64,

    /// L3 hit count
    pub l3_hit_count: u64,

    /// Miss count
    pub miss_count: u64,

    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
}

impl CacheStatistics {
    /// Total entries
    pub fn total_entries(&self) -> usize {
        self.l1_entries + self.l2_entries + self.l3_entries
    }

    /// Total accesses
    pub fn total_accesses(&self) -> u64 {
        self.l1_hit_count + self.l2_hit_count + self.l3_hit_count + self.miss_count
    }

    /// Hit rate (0.0 - 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_accesses();
        if total == 0 {
            return 0.0;
        }
        let hits = self.l1_hit_count + self.l2_hit_count + self.l3_hit_count;
        hits as f64 / total as f64
    }
}

/// LRU cache entry
struct CacheEntry {
    frame: VideoFrame,
    access_count: u64,
}

/// Multi-tier frame cache
pub struct Cache {
    config: CacheConfig,

    /// L1 (hot) cache - recent frames
    l1: RwLock<HashMap<i64, CacheEntry>>,

    /// L2 (keyframe) cache
    l2: RwLock<HashMap<i64, CacheEntry>>,

    /// L3 (cold) cache
    l3: RwLock<HashMap<i64, CacheEntry>>,

    /// L1 access order for LRU
    l1_order: RwLock<Vec<i64>>,

    /// L2 access order
    l2_order: RwLock<Vec<i64>>,

    /// L3 access order (SIEVE)
    l3_order: RwLock<Vec<i64>>,

    /// Statistics
    l1_hits: AtomicU64,
    l2_hits: AtomicU64,
    l3_hits: AtomicU64,
    misses: AtomicU64,
}

impl Cache {
    /// Create a new cache with configuration
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            l1: RwLock::new(HashMap::new()),
            l2: RwLock::new(HashMap::new()),
            l3: RwLock::new(HashMap::new()),
            l1_order: RwLock::new(Vec::new()),
            l2_order: RwLock::new(Vec::new()),
            l3_order: RwLock::new(Vec::new()),
            l1_hits: AtomicU64::new(0),
            l2_hits: AtomicU64::new(0),
            l3_hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Get frame from cache with tolerance
    pub fn get(&self, pts_us: i64, tolerance_us: i64) -> Option<VideoFrame> {
        // Check L1 first
        if let Some(frame) = self.get_from_l1(pts_us, tolerance_us) {
            self.l1_hits.fetch_add(1, Ordering::Relaxed);
            return Some(frame);
        }

        // Check L2
        if let Some(frame) = self.get_from_l2(pts_us, tolerance_us) {
            self.l2_hits.fetch_add(1, Ordering::Relaxed);
            // Promote to L1
            self.insert_l1(pts_us, frame.clone());
            return Some(frame);
        }

        // Check L3
        if let Some(frame) = self.get_from_l3(pts_us, tolerance_us) {
            self.l3_hits.fetch_add(1, Ordering::Relaxed);
            // Promote to L1
            self.insert_l1(pts_us, frame.clone());
            return Some(frame);
        }

        None
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Insert frame into L1 cache
    pub fn insert_l1(&self, pts_us: i64, frame: VideoFrame) {
        let mut cache = self.l1.write();
        let mut order = self.l1_order.write();

        // Evict if at capacity
        while cache.len() >= self.config.l1_capacity && !order.is_empty() {
            let oldest = order.remove(0);
            cache.remove(&oldest);
        }

        cache.insert(
            pts_us,
            CacheEntry {
                frame,
                access_count: 1,
            },
        );
        order.push(pts_us);
    }

    /// Insert keyframe into L2 cache
    pub fn insert_l2(&self, pts_us: i64, frame: VideoFrame) {
        if !frame.is_keyframe {
            return;
        }

        let mut cache = self.l2.write();
        let mut order = self.l2_order.write();

        while cache.len() >= self.config.l2_capacity && !order.is_empty() {
            let oldest = order.remove(0);
            cache.remove(&oldest);
        }

        cache.insert(
            pts_us,
            CacheEntry {
                frame,
                access_count: 1,
            },
        );
        order.push(pts_us);
    }

    /// Insert frame into L3 cache (cold storage)
    pub fn insert_l3(&self, pts_us: i64, frame: VideoFrame) {
        let mut cache = self.l3.write();
        let mut order = self.l3_order.write();

        // SIEVE eviction
        while cache.len() >= self.config.l3_capacity && !order.is_empty() {
            let oldest = order.remove(0);
            cache.remove(&oldest);
        }

        cache.insert(
            pts_us,
            CacheEntry {
                frame,
                access_count: 1,
            },
        );
        order.push(pts_us);
    }

    /// Get statistics
    pub fn statistics(&self) -> CacheStatistics {
        let l1 = self.l1.read();
        let l2 = self.l2.read();
        let l3 = self.l3.read();

        let memory = self.calculate_memory_usage(&l1, &l2, &l3);

        CacheStatistics {
            l1_entries: l1.len(),
            l2_entries: l2.len(),
            l3_entries: l3.len(),
            l1_hit_count: self.l1_hits.load(Ordering::Relaxed),
            l2_hit_count: self.l2_hits.load(Ordering::Relaxed),
            l3_hit_count: self.l3_hits.load(Ordering::Relaxed),
            miss_count: self.misses.load(Ordering::Relaxed),
            memory_usage_bytes: memory,
        }
    }

    /// Clear all caches
    pub fn clear(&self) {
        self.l1.write().clear();
        self.l2.write().clear();
        self.l3.write().clear();
        self.l1_order.write().clear();
        self.l2_order.write().clear();
        self.l3_order.write().clear();
    }

    // Private helpers

    fn get_from_l1(&self, pts_us: i64, tolerance_us: i64) -> Option<VideoFrame> {
        let cache = self.l1.read();
        self.find_in_cache(&cache, pts_us, tolerance_us)
    }

    fn get_from_l2(&self, pts_us: i64, tolerance_us: i64) -> Option<VideoFrame> {
        let cache = self.l2.read();
        self.find_in_cache(&cache, pts_us, tolerance_us)
    }

    fn get_from_l3(&self, pts_us: i64, tolerance_us: i64) -> Option<VideoFrame> {
        let cache = self.l3.read();
        self.find_in_cache(&cache, pts_us, tolerance_us)
    }

    fn find_in_cache(
        &self,
        cache: &HashMap<i64, CacheEntry>,
        pts_us: i64,
        tolerance_us: i64,
    ) -> Option<VideoFrame> {
        // Exact match first
        if let Some(entry) = cache.get(&pts_us) {
            return Some(entry.frame.clone());
        }

        // Search within tolerance
        let min = pts_us - tolerance_us;
        let max = pts_us + tolerance_us;

        cache
            .iter()
            .filter(|(&k, _)| k >= min && k <= max)
            .min_by_key(|(&k, _)| (k - pts_us).abs())
            .map(|(_, v)| v.frame.clone())
    }

    fn calculate_memory_usage(
        &self,
        l1: &HashMap<i64, CacheEntry>,
        l2: &HashMap<i64, CacheEntry>,
        l3: &HashMap<i64, CacheEntry>,
    ) -> u64 {
        let l1_mem: usize = l1.values().map(|e| e.frame.data.len()).sum();
        let l2_mem: usize = l2.values().map(|e| e.frame.data.len()).sum();
        let l3_mem: usize = l3.values().map(|e| e.frame.data.len()).sum();
        (l1_mem + l2_mem + l3_mem) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::config::PixelFormat;

    fn test_frame(pts_us: i64) -> VideoFrame {
        VideoFrame::new(
            vec![0u8; 1000],
            100,
            100,
            400,
            pts_us,
            16666,
            pts_us == 0,
            pts_us / 16666,
            PixelFormat::Bgra,
        )
    }

    #[test]
    fn test_cache_insert_get() {
        let cache = Cache::new(CacheConfig::default());

        let frame = test_frame(0);
        cache.insert_l1(0, frame);

        let result = cache.get(0, 1000);
        assert!(result.is_some());
    }

    #[test]
    fn test_cache_tolerance() {
        let cache = Cache::new(CacheConfig::default());

        cache.insert_l1(1000, test_frame(1000));

        // Should find within tolerance
        let result = cache.get(1500, 1000);
        assert!(result.is_some());

        // Should not find outside tolerance
        let result = cache.get(5000, 1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_lru_eviction() {
        let config = CacheConfig {
            l1_capacity: 3,
            ..Default::default()
        };
        let cache = Cache::new(config);

        cache.insert_l1(0, test_frame(0));
        cache.insert_l1(1000, test_frame(1000));
        cache.insert_l1(2000, test_frame(2000));
        cache.insert_l1(3000, test_frame(3000)); // Should evict 0

        assert!(cache.get(0, 0).is_none());
        assert!(cache.get(3000, 0).is_some());
    }

    #[test]
    fn test_statistics() {
        let cache = Cache::new(CacheConfig::default());

        cache.insert_l1(0, test_frame(0));
        cache.get(0, 0); // Hit
        cache.get(1000, 0); // Miss
        cache.record_miss();

        let stats = cache.statistics();
        assert_eq!(stats.l1_entries, 1);
        assert_eq!(stats.l1_hit_count, 1);
        assert_eq!(stats.miss_count, 1);
    }
}
