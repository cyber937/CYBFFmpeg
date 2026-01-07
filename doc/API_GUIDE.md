# API Guide — CYBFFmpeg

## Quick Start

### Creating a Decoder

```swift
import CYBFFmpeg

let url = URL(fileURLWithPath: "/path/to/video.webm")
let decoder = try FFmpegDecoder(url: url)
try await decoder.prepare()
```

### Accessing Media Info

```swift
let info = decoder.mediaInfo

print("Duration: \(info.duration)s")
print("Format: \(info.containerFormat)")

for track in info.videoTracks {
    print("Video: \(track.codec.longName) \(track.width)x\(track.height)")
    print("Hardware decode: \(track.isHardwareDecodable)")
}

for track in info.audioTracks {
    print("Audio: \(track.codec.longName) \(track.sampleRate)Hz")
}
```

## Frame Access

### Single Frame

```swift
// Get frame at specific time (with tolerance)
if let frame = try decoder.getFrame(at: 5.0, tolerance: 0.016) {
    let pixelBuffer = frame.pixelBuffer
    // Render to Metal, save to file, etc.
}
```

### Sequential Frames

```swift
// Start decoding
decoder.startDecoding()

// Get next frames
while let frame = decoder.getNextFrame() {
    process(frame)

    if frame.presentationTime > 10.0 {
        break
    }
}

decoder.stopDecoding()
```

### Seeking

```swift
// Seek to specific time
if let frame = try decoder.seek(to: 30.0) {
    // Frame at or near 30 seconds
    print("Seeked to: \(frame.presentationTime)s")
}
```

## Scrubbing (High-Speed Navigation)

### Enable Prefetch

```swift
// User starts scrubbing forward
decoder.startPrefetch(direction: 1, velocity: 2.0)

// During scrub, get frames
while userIsScrubbing {
    let time = getCurrentScrubTime()
    if let frame = try? decoder.getFrame(at: time, tolerance: 0.033) {
        display(frame)
    }
}

// User stops scrubbing
decoder.stopPrefetch()
```

### Direction and Velocity

| Parameter | Description |
|-----------|-------------|
| `direction` | 1 = forward, -1 = backward |
| `velocity` | Scrub speed multiplier (1.0 = normal) |

```swift
// Fast forward scrub
decoder.startPrefetch(direction: 1, velocity: 5.0)

// Reverse scrub
decoder.startPrefetch(direction: -1, velocity: 2.0)
```

## Cache Management

### Check Statistics

```swift
let stats = decoder.cacheStatistics

print("Entries: L1=\(stats.l1Entries) L2=\(stats.l2Entries) L3=\(stats.l3Entries)")
print("Hit rate: \(String(format: "%.1f%%", stats.hitRate * 100))")
print("Memory: \(stats.memoryUsageBytes / 1024 / 1024) MB")
```

### Performance Tuning

```swift
// High-performance configuration for 4K scrubbing
let config = DecoderConfiguration(
    preferHardwareDecoding: true,
    cacheConfiguration: CacheConfiguration(
        l1Capacity: 60,      // More hot frames
        l2Capacity: 200,     // More keyframes
        l3Capacity: 1000,    // Larger cold cache
        enablePrefetch: true
    ),
    threadCount: 8,
    outputPixelFormat: .bgra
)

let decoder = try FFmpegDecoder(url: url, configuration: config)
```

## Hardware Decoding

### Check Support

```swift
let info = decoder.mediaInfo

for track in info.videoTracks {
    if track.isHardwareDecodable {
        print("\(track.codec.name): VideoToolbox available")
    } else {
        print("\(track.codec.name): Software decode")
    }
}
```

### Force Software Decoding

```swift
let config = DecoderConfiguration(
    preferHardwareDecoding: false,  // Disable VideoToolbox
    // ...
)
```

## Error Handling

### Comprehensive Error Handling

```swift
do {
    let decoder = try FFmpegDecoder(url: url)
    try await decoder.prepare()

    if let frame = try decoder.getFrame(at: 0.0, tolerance: 0.016) {
        display(frame)
    }
} catch FFmpegError.fileNotFound(let url) {
    showError("File not found: \(url.lastPathComponent)")

} catch FFmpegError.codecNotSupported(let codec) {
    showError("Unsupported codec: \(codec)")

} catch FFmpegError.invalidFormat(let format) {
    showError("Invalid format: \(format)")

} catch FFmpegError.decodeFailed(let reason) {
    showError("Decode failed: \(reason)")

} catch FFmpegError.seekFailed(let time) {
    showError("Cannot seek to \(time)s")

} catch FFmpegError.memoryError {
    showError("Out of memory")

} catch {
    showError("Unknown error: \(error)")
}
```

## Lifecycle Management

### Proper Cleanup

```swift
class VideoController {
    private var decoder: FFmpegDecoder?

    func loadVideo(_ url: URL) async throws {
        // Clean up previous decoder
        decoder?.invalidate()
        decoder = nil

        // Create new decoder
        decoder = try FFmpegDecoder(url: url)
        try await decoder?.prepare()
    }

    deinit {
        decoder?.invalidate()
    }
}
```

### Actor-based Usage

```swift
actor VideoDecodeManager {
    private var decoders: [URL: FFmpegDecoder] = [:]

    func getDecoder(for url: URL) async throws -> FFmpegDecoder {
        if let existing = decoders[url] {
            return existing
        }

        let decoder = try FFmpegDecoder(url: url)
        try await decoder.prepare()
        decoders[url] = decoder
        return decoder
    }

    func release(for url: URL) {
        decoders[url]?.invalidate()
        decoders[url] = nil
    }

    func releaseAll() {
        for decoder in decoders.values {
            decoder.invalidate()
        }
        decoders.removeAll()
    }
}
```

## Integration Examples

### With Metal Rendering

```swift
func renderFrame(to metalView: MTKView, at time: Double) {
    guard let frame = try? decoder.getFrame(at: time, tolerance: 0.016) else {
        return
    }

    // Create texture from CVPixelBuffer
    var textureRef: CVMetalTexture?
    let width = CVPixelBufferGetWidth(frame.pixelBuffer)
    let height = CVPixelBufferGetHeight(frame.pixelBuffer)

    CVMetalTextureCacheCreateTextureFromImage(
        nil,
        textureCache,
        frame.pixelBuffer,
        nil,
        .bgra8Unorm,
        width,
        height,
        0,
        &textureRef
    )

    if let texture = textureRef.flatMap({ CVMetalTextureGetTexture($0) }) {
        // Render texture
    }
}
```

### With CYBMediaHolder (Optional)

```swift
import CYBMediaHolder
import CYBFFmpeg

// In CYBMediaHolder adapter
extension FFmpegMediaInfo {
    func toMediaDescriptor() -> MediaDescriptor {
        return MediaDescriptor(
            containerFormat: containerFormat,
            durationSeconds: duration,
            fileName: url.lastPathComponent,
            videoTracks: videoTracks.map { $0.toVideoTrackDescriptor() },
            audioTracks: audioTracks.map { $0.toAudioTrackDescriptor() },
            probeBackend: "ffmpeg"
        )
    }
}
```

## Best Practices

### 1. Always Prepare Before Use

```swift
// GOOD
let decoder = try FFmpegDecoder(url: url)
try await decoder.prepare()  // Required!
let frame = try decoder.getFrame(at: 0, tolerance: 0.016)

// BAD - will fail
let decoder = try FFmpegDecoder(url: url)
let frame = try decoder.getFrame(at: 0, tolerance: 0.016)  // Error!
```

### 2. Use Appropriate Tolerance

```swift
// For playback (frame-accurate)
let frame = try decoder.getFrame(at: time, tolerance: 0.001)

// For scrubbing (faster, allows approximation)
let frame = try decoder.getFrame(at: time, tolerance: 0.033)

// For thumbnails (any nearby frame)
let frame = try decoder.getFrame(at: time, tolerance: 1.0)
```

### 3. Enable Prefetch for Scrubbing

```swift
// Always use prefetch when scrubbing
decoder.startPrefetch(direction: scrubDirection, velocity: scrubSpeed)
// ... scrub operations ...
decoder.stopPrefetch()
```

### 4. Monitor Cache Performance

```swift
// Check periodically during heavy usage
let stats = decoder.cacheStatistics
if stats.hitRate < 0.5 {
    // Consider increasing cache size or adjusting prefetch
    print("Warning: Low cache hit rate \(stats.hitRate)")
}
```

### 5. Release Resources Properly

```swift
// Always invalidate when done
defer {
    decoder.invalidate()
}
```
