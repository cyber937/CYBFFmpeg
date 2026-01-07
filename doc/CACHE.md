# Cache System — CYBFFmpeg

CYBFFmpeg provides a multi-tier caching system implemented in Rust for high-performance frame access and scrubbing.

## Goals

- Sub-100ms scrub response time
- Efficient memory usage with intelligent eviction
- Predictable performance under heavy load
- Support for both forward and reverse navigation

## Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                    Cache Manager (Rust)                      │
│                                                              │
├─────────────────────────────────────────────────────────────┤
│  L1: Hot Cache                                               │
│  - Capacity: 30 frames (configurable)                        │
│  - Strategy: LRU eviction                                    │
│  - Access: O(1)                                              │
│  - Purpose: Immediate playhead vicinity                      │
├─────────────────────────────────────────────────────────────┤
│  L2: Keyframe Cache                                          │
│  - Capacity: 100 frames (configurable)                       │
│  - Strategy: Keyframes only                                  │
│  - Access: O(1)                                              │
│  - Purpose: Quick seeking anchors                            │
├─────────────────────────────────────────────────────────────┤
│  L3: Cold Cache                                              │
│  - Capacity: 500 frames (configurable)                       │
│  - Strategy: SIEVE eviction                                  │
│  - Access: O(1)                                              │
│  - Purpose: Background prefetch storage                      │
└─────────────────────────────────────────────────────────────┘
```

## Cache Tiers

### L1: Hot Cache

Stores recently accessed frames near the current playhead.

| Property | Value |
|----------|-------|
| Default Capacity | 30 frames |
| Eviction | LRU (Least Recently Used) |
| Access Time | <1ms |
| Content | All frame types |

**Behavior:**
- Populated on every frame access
- Evicts oldest accessed frame when full
- Cleared on seek outside range

### L2: Keyframe Cache

Stores keyframes (I-frames) for quick seeking.

| Property | Value |
|----------|-------|
| Default Capacity | 100 frames |
| Eviction | LRU |
| Access Time | <1ms |
| Content | Keyframes only |

**Behavior:**
- Populated during prefetch and decoding
- Used as anchors for seeking
- Essential for GOP-based codecs (H.264, H.265, VP9)

### L3: Cold Cache

Large background cache for prefetched frames.

| Property | Value |
|----------|-------|
| Default Capacity | 500 frames |
| Eviction | SIEVE (smart LRU variant) |
| Access Time | <1ms |
| Content | All frame types |

**Behavior:**
- Populated by prefetch threads
- Promotes to L1 on access
- Uses SIEVE for better eviction decisions

## Cache Strategy

### Write Policy

```
Frame Decoded
    ↓
Is Keyframe? ─Yes→ Write to L2
    ↓ No
Write to L1
    ↓
If prefetching → Also write to L3
```

### Read Policy

```
Frame Request
    ↓
Check L1 ─Hit→ Return (no promotion)
    ↓ Miss
Check L2 ─Hit→ Promote to L1 → Return
    ↓ Miss
Check L3 ─Hit→ Promote to L1 → Return
    ↓ Miss
Decode frame → Write to L1 → Return
```

### Prefetch Behavior

When prefetching is active:

```
Prefetch Direction: Forward (direction = 1)
┌──────────────────────────────────────────────┐
│  [decoded] [decoded] [current] [prefetch...] │
│                         ↑                    │
│                    playhead                  │
└──────────────────────────────────────────────┘

Prefetch Direction: Backward (direction = -1)
┌──────────────────────────────────────────────┐
│  [...prefetch] [current] [decoded] [decoded] │
│                    ↑                         │
│               playhead                       │
└──────────────────────────────────────────────┘
```

## Configuration

### Default Configuration

```swift
let config = CacheConfiguration.default
// l1Capacity: 30
// l2Capacity: 100
// l3Capacity: 500
// enablePrefetch: true
```

### Performance Configuration

```swift
let config = CacheConfiguration(
    l1Capacity: 60,      // More hot frames
    l2Capacity: 200,     // More keyframes
    l3Capacity: 1000,    // Larger cold cache
    enablePrefetch: true
)
```

### Low Memory Configuration

```swift
let config = CacheConfiguration(
    l1Capacity: 15,      // Minimal hot frames
    l2Capacity: 50,      // Fewer keyframes
    l3Capacity: 100,     // Small cold cache
    enablePrefetch: false // Disable prefetch
)
```

### Custom Configuration

```swift
let config = DecoderConfiguration(
    preferHardwareDecoding: true,
    cacheConfiguration: CacheConfiguration(
        l1Capacity: 45,
        l2Capacity: 150,
        l3Capacity: 750,
        enablePrefetch: true
    ),
    threadCount: 4,
    outputPixelFormat: .bgra
)

let decoder = try FFmpegDecoder(url: url, configuration: config)
```

## Memory Estimation

### Per-Frame Memory

| Resolution | Pixel Format | Size |
|------------|--------------|------|
| 1080p | BGRA | ~8 MB |
| 1080p | NV12 | ~3 MB |
| 4K | BGRA | ~33 MB |
| 4K | NV12 | ~12 MB |

### Total Cache Memory

Default configuration with 1080p BGRA:
- L1 (30 frames): ~240 MB
- L2 (100 frames): ~800 MB
- L3 (500 frames): ~4 GB
- **Total**: ~5 GB (worst case)

Note: L2 and L3 share keyframes, actual usage is typically lower.

## Statistics

### Accessing Statistics

```swift
let stats = decoder.cacheStatistics

print("L1 entries: \(stats.l1Entries)")
print("L2 entries: \(stats.l2Entries)")
print("L3 entries: \(stats.l3Entries)")
print("Total: \(stats.totalEntries)")

print("Hit counts: L1=\(stats.l1HitCount) L2=\(stats.l2HitCount) L3=\(stats.l3HitCount)")
print("Miss count: \(stats.missCount)")
print("Hit rate: \(String(format: "%.1f%%", stats.hitRate * 100))")
print("Memory: \(stats.memoryUsageBytes / 1024 / 1024) MB")
```

### CacheStatistics Structure

```swift
public struct CacheStatistics: Sendable {
    // Entry counts
    public let l1Entries: Int
    public let l2Entries: Int
    public let l3Entries: Int
    public var totalEntries: Int { l1Entries + l2Entries + l3Entries }

    // Hit/miss tracking
    public let l1HitCount: Int
    public let l2HitCount: Int
    public let l3HitCount: Int
    public let missCount: Int

    // Computed metrics
    public var totalAccesses: Int
    public var hitRate: Double
    public var l1HitRate: Double
    public var l2HitRate: Double
    public var l3HitRate: Double

    // Memory
    public let memoryUsageBytes: Int
}
```

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| L1 hit access | <1ms | O(1) hash lookup |
| L2 hit access | <1ms | O(1) hash lookup |
| L3 hit access | <1ms | O(1) hash lookup |
| Cache miss | <16ms | Full frame decode |
| Scrub response | <100ms | L2 keyframe seek |
| Overall hit rate | >80% | During normal use |

## SIEVE Eviction (L3)

L3 uses SIEVE eviction, a modern variant of LRU:

### Algorithm

```
SIEVE tracks:
- visited bit for each entry
- hand pointer (circular)

On eviction:
1. Move hand to find entry with visited=false
2. Reset visited=true entries to visited=false as hand passes
3. Evict entry with visited=false

On access:
1. Set visited=true for accessed entry
```

### Advantages

- Better hit rate than LRU for certain patterns
- Resists scan pollution
- Low overhead (single bit per entry)

## Prefetch System

### Starting Prefetch

```swift
// User starts scrubbing forward
decoder.startPrefetch(direction: 1, velocity: 2.0)
```

### Parameters

| Parameter | Description | Values |
|-----------|-------------|--------|
| `direction` | Navigation direction | 1 (forward), -1 (backward) |
| `velocity` | Speed multiplier | 1.0 (normal), 2.0 (2x speed), etc. |

### Prefetch Threads

The Rust core spawns prefetch worker threads:

```
Main Thread: User requests → L1 lookup → Return
            ↓ (cache miss)
Decode Thread: Decode frame → Write L1
            ↓
Prefetch Threads (1-4): Predictive decode → Write L3
```

### Stopping Prefetch

```swift
// User stops scrubbing
decoder.stopPrefetch()
```

This signals prefetch threads to stop and conserve resources.

## Best Practices

### 1. Enable Prefetch for Scrubbing

```swift
func onScrubStart(direction: Int, speed: Double) {
    decoder.startPrefetch(direction: direction, velocity: speed)
}

func onScrubEnd() {
    decoder.stopPrefetch()
}
```

### 2. Monitor Hit Rate

```swift
// Log periodically during development
let stats = decoder.cacheStatistics
if stats.hitRate < 0.5 {
    print("Warning: Cache hit rate low (\(stats.hitRate))")
    print("Consider increasing cache size")
}
```

### 3. Adjust for Content Type

```swift
// All-intra codec (ProRes) - smaller L2 needed
let proResConfig = CacheConfiguration(
    l1Capacity: 30,
    l2Capacity: 30,   // Every frame is a keyframe
    l3Capacity: 500,
    enablePrefetch: true
)

// Long-GOP codec (H.264) - larger L2 needed
let h264Config = CacheConfiguration(
    l1Capacity: 30,
    l2Capacity: 200,  // Need more keyframe anchors
    l3Capacity: 500,
    enablePrefetch: true
)
```

### 4. Release Memory When Not Needed

```swift
// When switching to different media
decoder.invalidate()  // Releases all cache memory
```

## Thread Safety

All cache operations are thread-safe:

- Cache access protected by `RwLock`
- Prefetch uses `crossbeam` channels
- Statistics use atomic counters
- No blocking on hot path

```rust
// Rust implementation uses:
use parking_lot::RwLock;
use crossbeam::channel;
use std::sync::atomic::AtomicU64;
```
