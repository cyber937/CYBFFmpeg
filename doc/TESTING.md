# Testing — CYBFFmpeg

This document describes the testing strategy for CYBFFmpeg.

## Test Architecture

```
Tests/
├── CYBFFmpegTests/           # Swift E2E tests
│   ├── MediaInfoTests.swift
│   ├── DecoderTests.swift
│   ├── FrameAccessTests.swift
│   ├── CacheTests.swift
│   ├── ScrubTests.swift
│   ├── ErrorHandlingTests.swift
│   └── PerformanceTests.swift
│
└── cyb-ffmpeg-core/tests/    # Rust unit tests
    ├── decoder_tests.rs
    ├── cache_tests.rs
    ├── threading_tests.rs
    └── ffi_tests.rs
```

## Swift Tests

### Running Tests

```bash
# Run all tests
swift test

# Run specific test
swift test --filter MediaInfoTests

# Run with verbose output
swift test -v
```

### MediaInfoTests.swift

```swift
import XCTest
@testable import CYBFFmpeg

final class MediaInfoTests: XCTestCase {

    func testLoadVP9MediaInfo() async throws {
        let url = Bundle.module.url(forResource: "sample_vp9", withExtension: "webm")!
        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()

        let info = decoder.mediaInfo
        XCTAssertEqual(info.containerFormat, "matroska,webm")
        XCTAssertGreaterThan(info.duration, 0)
        XCTAssertFalse(info.videoTracks.isEmpty)

        let videoTrack = info.videoTracks[0]
        XCTAssertEqual(videoTrack.codec.name, "vp9")
        XCTAssertGreaterThan(videoTrack.width, 0)
        XCTAssertGreaterThan(videoTrack.height, 0)
    }

    func testLoadAV1MediaInfo() async throws {
        let url = Bundle.module.url(forResource: "sample_av1", withExtension: "mp4")!
        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()

        let info = decoder.mediaInfo
        let videoTrack = info.videoTracks[0]
        XCTAssertEqual(videoTrack.codec.name, "av1")
    }

    func testHardwareDecodableFlag() async throws {
        // H.264 should be hardware decodable
        let h264URL = Bundle.module.url(forResource: "sample_h264", withExtension: "mp4")!
        let h264Decoder = try FFmpegDecoder(url: h264URL)
        try await h264Decoder.prepare()
        XCTAssertTrue(h264Decoder.mediaInfo.videoTracks[0].isHardwareDecodable)

        // AV1 may not be hardware decodable on all systems
        let av1URL = Bundle.module.url(forResource: "sample_av1", withExtension: "mp4")!
        let av1Decoder = try FFmpegDecoder(url: av1URL)
        try await av1Decoder.prepare()
        // Just check it doesn't crash - HW support varies
        _ = av1Decoder.mediaInfo.videoTracks[0].isHardwareDecodable
    }
}
```

### DecoderTests.swift

```swift
import XCTest
@testable import CYBFFmpeg

final class DecoderTests: XCTestCase {

    var decoder: FFmpegDecoder!

    override func setUp() async throws {
        let url = Bundle.module.url(forResource: "sample_vp9", withExtension: "webm")!
        decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()
    }

    override func tearDown() {
        decoder?.invalidate()
        decoder = nil
    }

    func testGetFrameAtTime() throws {
        let frame = try decoder.getFrame(at: 0.0, tolerance: 0.016)
        XCTAssertNotNil(frame)

        if let frame = frame {
            XCTAssertGreaterThan(frame.width, 0)
            XCTAssertGreaterThan(frame.height, 0)
            XCTAssertNotNil(frame.pixelBuffer)
        }
    }

    func testSeek() throws {
        let frame = try decoder.seek(to: 5.0)
        XCTAssertNotNil(frame)

        if let frame = frame {
            XCTAssertGreaterThanOrEqual(frame.presentationTime, 4.9)
            XCTAssertLessThanOrEqual(frame.presentationTime, 5.1)
        }
    }

    func testSequentialFrames() throws {
        decoder.startDecoding()

        var frameCount = 0
        var lastPTS: Double = -1

        while let frame = decoder.getNextFrame(), frameCount < 30 {
            XCTAssertGreaterThan(frame.presentationTime, lastPTS)
            lastPTS = frame.presentationTime
            frameCount += 1
        }

        XCTAssertEqual(frameCount, 30)
        decoder.stopDecoding()
    }

    func testInvalidate() throws {
        decoder.invalidate()

        XCTAssertThrowsError(try decoder.getFrame(at: 0, tolerance: 0.016))
    }
}
```

### CacheTests.swift

```swift
import XCTest
@testable import CYBFFmpeg

final class CacheTests: XCTestCase {

    func testCacheHitRate() async throws {
        let url = Bundle.module.url(forResource: "sample_vp9", withExtension: "webm")!
        let config = DecoderConfiguration(
            preferHardwareDecoding: true,
            cacheConfiguration: CacheConfiguration(
                l1Capacity: 30,
                l2Capacity: 100,
                l3Capacity: 500,
                enablePrefetch: true
            ),
            threadCount: 4,
            outputPixelFormat: .bgra
        )

        let decoder = try FFmpegDecoder(url: url, configuration: config)
        try await decoder.prepare()

        // Access same frames multiple times
        for time in stride(from: 0.0, to: 5.0, by: 0.033) {
            _ = try? decoder.getFrame(at: time, tolerance: 0.016)
        }

        // Re-access (should hit cache)
        for time in stride(from: 0.0, to: 5.0, by: 0.033) {
            _ = try? decoder.getFrame(at: time, tolerance: 0.016)
        }

        let stats = decoder.cacheStatistics
        XCTAssertGreaterThan(stats.hitRate, 0.5, "Cache hit rate should be > 50%")
    }

    func testCacheStatistics() async throws {
        let url = Bundle.module.url(forResource: "sample_vp9", withExtension: "webm")!
        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()

        // Access some frames
        for _ in 0..<10 {
            _ = decoder.getNextFrame()
        }

        let stats = decoder.cacheStatistics
        XCTAssertGreaterThan(stats.totalEntries, 0)
        XCTAssertGreaterThanOrEqual(stats.memoryUsageBytes, 0)
    }
}
```

### PerformanceTests.swift

```swift
import XCTest
@testable import CYBFFmpeg

final class PerformanceTests: XCTestCase {

    func testFrameDecodePerformance() async throws {
        let url = Bundle.module.url(forResource: "sample_1080p", withExtension: "webm")!
        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()

        measure {
            for time in stride(from: 0.0, to: 10.0, by: 0.033) {
                _ = try? decoder.getFrame(at: time, tolerance: 0.016)
            }
        }

        // Target: <16ms per frame for 60fps
    }

    func testScrubPerformance() async throws {
        let url = Bundle.module.url(forResource: "sample_1080p", withExtension: "webm")!
        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()

        decoder.startPrefetch(direction: 1, velocity: 2.0)

        let start = CFAbsoluteTimeGetCurrent()

        // Simulate scrubbing
        for time in stride(from: 0.0, to: 30.0, by: 0.5) {
            _ = try? decoder.getFrame(at: time, tolerance: 0.033)
        }

        let elapsed = CFAbsoluteTimeGetCurrent() - start
        let perSeek = elapsed / 60.0 * 1000 // ms per seek

        decoder.stopPrefetch()

        XCTAssertLessThan(perSeek, 100, "Scrub response should be <100ms")
    }

    func testMemoryUsage() async throws {
        let url = Bundle.module.url(forResource: "sample_4k", withExtension: "webm")!
        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()

        // Access frames
        for _ in 0..<100 {
            _ = decoder.getNextFrame()
        }

        let stats = decoder.cacheStatistics
        let memoryMB = stats.memoryUsageBytes / 1024 / 1024

        // 4K BGRA: ~33MB per frame
        // L1 (30) + L2 (100) + L3 (500) worst case: ~20GB
        // Actual should be much less due to sharing
        XCTAssertLessThan(memoryMB, 2048, "Memory should be <2GB")
    }
}
```

## Rust Tests

### Running Tests

```bash
cd cyb-ffmpeg-core
cargo test

# Run specific test
cargo test decoder_tests

# With output
cargo test -- --nocapture
```

### decoder_tests.rs

```rust
use crate::decoder::{Decoder, DecoderConfig};
use std::path::Path;

#[test]
fn test_decoder_create() {
    let path = Path::new("tests/fixtures/sample_vp9.webm");
    if !path.exists() {
        eprintln!("Skipping test: fixture not found");
        return;
    }

    let config = DecoderConfig::default();
    let decoder = Decoder::new(path, config);
    assert!(decoder.is_ok());
}

#[test]
fn test_decoder_prepare() {
    let path = Path::new("tests/fixtures/sample_vp9.webm");
    if !path.exists() {
        return;
    }

    let mut decoder = Decoder::new(path, DecoderConfig::default()).unwrap();
    let result = decoder.prepare();
    assert!(result.is_ok());

    let info = decoder.media_info();
    assert!(info.duration > 0.0);
    assert!(!info.video_tracks.is_empty());
}

#[test]
fn test_get_frame() {
    let path = Path::new("tests/fixtures/sample_vp9.webm");
    if !path.exists() {
        return;
    }

    let mut decoder = Decoder::new(path, DecoderConfig::default()).unwrap();
    decoder.prepare().unwrap();

    let frame = decoder.get_frame_at(0, 16_000); // 0s, 16ms tolerance
    assert!(frame.is_ok());

    let frame = frame.unwrap();
    assert!(frame.is_some());

    let frame = frame.unwrap();
    assert!(frame.width > 0);
    assert!(frame.height > 0);
}
```

### cache_tests.rs

```rust
use crate::cache::{Cache, CacheConfig, FrameData};

#[test]
fn test_l1_cache_basic() {
    let config = CacheConfig {
        l1_capacity: 10,
        l2_capacity: 0,
        l3_capacity: 0,
        enable_prefetch: false,
    };

    let cache = Cache::new(config);

    // Insert frames
    for i in 0..10 {
        let frame = FrameData::test_frame(i);
        cache.insert_l1(i, frame);
    }

    // Should hit
    for i in 0..10 {
        assert!(cache.get(i).is_some());
    }

    // Insert one more (should evict oldest)
    cache.insert_l1(10, FrameData::test_frame(10));

    // Frame 0 should be evicted
    assert!(cache.get(0).is_none());
    assert!(cache.get(10).is_some());
}

#[test]
fn test_lru_eviction() {
    let config = CacheConfig {
        l1_capacity: 5,
        ..Default::default()
    };

    let cache = Cache::new(config);

    // Insert 0-4
    for i in 0..5 {
        cache.insert_l1(i, FrameData::test_frame(i));
    }

    // Access 0,1,2 (making them recently used)
    cache.get(0);
    cache.get(1);
    cache.get(2);

    // Insert 5,6 (should evict 3,4)
    cache.insert_l1(5, FrameData::test_frame(5));
    cache.insert_l1(6, FrameData::test_frame(6));

    // 0,1,2 should still exist
    assert!(cache.get(0).is_some());
    assert!(cache.get(1).is_some());
    assert!(cache.get(2).is_some());

    // 3,4 should be evicted
    assert!(cache.get(3).is_none());
    assert!(cache.get(4).is_none());
}

#[test]
fn test_cache_statistics() {
    let cache = Cache::new(CacheConfig::default());

    // Generate some hits and misses
    cache.insert_l1(0, FrameData::test_frame(0));
    cache.get(0); // Hit
    cache.get(0); // Hit
    cache.get(1); // Miss

    let stats = cache.statistics();
    assert_eq!(stats.l1_hit_count, 2);
    assert_eq!(stats.miss_count, 1);
}
```

### threading_tests.rs

```rust
use crate::threading::PrefetchManager;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_prefetch_start_stop() {
    let manager = PrefetchManager::new(4);

    manager.start(1, 2.0);
    thread::sleep(Duration::from_millis(100));

    assert!(manager.is_running());

    manager.stop();
    thread::sleep(Duration::from_millis(50));

    assert!(!manager.is_running());
}

#[test]
fn test_prefetch_direction_change() {
    let manager = PrefetchManager::new(4);

    manager.start(1, 1.0);
    thread::sleep(Duration::from_millis(50));

    manager.start(-1, 1.0); // Change direction
    thread::sleep(Duration::from_millis(50));

    assert!(manager.is_running());

    manager.stop();
}
```

## Test Fixtures

### Sample Files

Store test media files in:
- Swift: `Tests/CYBFFmpegTests/Resources/`
- Rust: `cyb-ffmpeg-core/tests/fixtures/`

Required fixtures:
- `sample_vp9.webm` - VP9 codec
- `sample_av1.mp4` - AV1 codec
- `sample_h264.mp4` - H.264 codec
- `sample_1080p.webm` - 1080p for performance tests
- `sample_4k.webm` - 4K for memory tests

### Generating Test Files

```bash
# VP9 WebM
ffmpeg -f lavfi -i testsrc=duration=10:size=1920x1080:rate=30 \
  -c:v libvpx-vp9 -b:v 2M sample_vp9.webm

# AV1 MP4
ffmpeg -f lavfi -i testsrc=duration=10:size=1920x1080:rate=30 \
  -c:v libaom-av1 -crf 30 sample_av1.mp4

# H.264 MP4
ffmpeg -f lavfi -i testsrc=duration=10:size=1920x1080:rate=30 \
  -c:v libx264 -preset ultrafast sample_h264.mp4
```

## CI Integration

### GitHub Actions

```yaml
name: Test

on: [push, pull_request]

jobs:
  swift-tests:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4

      - name: Build FFmpeg
        run: ./ffmpeg-build/scripts/build-ffmpeg.sh

      - name: Run Swift tests
        run: swift test

  rust-tests:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build FFmpeg
        run: ./ffmpeg-build/scripts/build-ffmpeg.sh

      - name: Run Rust tests
        run: |
          cd cyb-ffmpeg-core
          cargo test
```

## Performance Benchmarks

### Target Metrics

| Metric | Target | Measured |
|--------|--------|----------|
| Frame decode (1080p) | <16ms | TBD |
| Frame decode (4K) | <33ms | TBD |
| Scrub response | <100ms | TBD |
| Cache hit rate | >80% | TBD |
| Memory (1080p) | <500MB | TBD |
| Memory (4K) | <2GB | TBD |

### Running Benchmarks

```bash
# Swift performance tests
swift test --filter Performance

# Rust benchmarks
cd cyb-ffmpeg-core
cargo bench
```
