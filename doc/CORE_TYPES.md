# Core Types — CYBFFmpeg

This document describes the fundamental types in CYBFFmpeg.

## FFmpegMediaInfo

Complete media information extracted by FFmpeg:

```swift
public struct FFmpegMediaInfo: Sendable {
    public let url: URL
    public let duration: Double
    public let containerFormat: String
    public let videoTracks: [FFmpegVideoTrack]
    public let audioTracks: [FFmpegAudioTrack]
    public let metadata: [String: String]
}
```

### Fields

| Field | Description |
|-------|-------------|
| `url` | Source media URL |
| `duration` | Duration in seconds |
| `containerFormat` | Container format (e.g., "matroska", "mp4") |
| `videoTracks` | Array of video track information |
| `audioTracks` | Array of audio track information |
| `metadata` | Container-level metadata |

### Usage

```swift
let decoder = try FFmpegDecoder(url: fileURL)
try await decoder.prepare()

let info = decoder.mediaInfo
print("Duration: \(info.duration)s")
print("Format: \(info.containerFormat)")
print("Video tracks: \(info.videoTracks.count)")
```

## FFmpegVideoTrack

Video track information:

```swift
public struct FFmpegVideoTrack: Sendable {
    public let index: Int
    public let codec: FFmpegCodec
    public let width: Int
    public let height: Int
    public let frameRate: Double
    public let bitRate: Int64?
    public let pixelFormat: String
    public let isHardwareDecodable: Bool

    // Color information
    public let colorSpace: String?
    public let colorPrimaries: String?
    public let colorTransfer: String?
    public let colorRange: ColorRange
}

public enum ColorRange: String, Sendable {
    case full
    case limited
    case unknown
}
```

### Fields

| Field | Description |
|-------|-------------|
| `index` | Track index in container |
| `codec` | Codec information |
| `width` / `height` | Video dimensions |
| `frameRate` | Frame rate (fps) |
| `bitRate` | Bit rate in bps (if available) |
| `pixelFormat` | FFmpeg pixel format (e.g., "yuv420p") |
| `isHardwareDecodable` | VideoToolbox supported |

### Hardware Decoding Check

```swift
if track.isHardwareDecodable {
    // VideoToolbox will be used
    print("Hardware decoding available")
} else {
    // Software decoding via FFmpeg
    print("Software decoding")
}
```

## FFmpegAudioTrack

Audio track information:

```swift
public struct FFmpegAudioTrack: Sendable {
    public let index: Int
    public let codec: FFmpegCodec
    public let sampleRate: Int
    public let channels: Int
    public let channelLayout: String?
    public let bitRate: Int64?
    public let languageCode: String?
}
```

### Fields

| Field | Description |
|-------|-------------|
| `index` | Track index |
| `codec` | Codec information |
| `sampleRate` | Sample rate (Hz) |
| `channels` | Number of channels |
| `channelLayout` | Layout string (e.g., "stereo", "5.1") |
| `bitRate` | Bit rate (if available) |
| `languageCode` | ISO 639 language code |

## FFmpegCodec

Codec identification:

```swift
public struct FFmpegCodec: Sendable {
    public let name: String       // Short name: "vp9", "av1", "h264"
    public let longName: String   // Full name: "Google VP9"
    public let fourCC: String?    // FourCC code: "vp09"
    public let isDecoder: Bool    // true for decoders
}
```

### Supported Codecs

| Name | Long Name | FourCC | Hardware |
|------|-----------|--------|----------|
| `h264` | H.264 / AVC | `avc1` | Yes |
| `hevc` | H.265 / HEVC | `hvc1` | Yes |
| `vp9` | Google VP9 | `vp09` | Yes (macOS 11+) |
| `av1` | AV1 (AOMedia) | `av01` | No |
| `mpeg2video` | MPEG-2 | - | No |
| `mpeg4` | MPEG-4 Part 2 | - | No |
| `dnxhd` | DNxHD / DNxHR | `AVdh` | No |
| `prores` | Apple ProRes | `apch` | Yes |

## FFmpegFrame

Decoded frame data:

```swift
public struct FFmpegFrame: @unchecked Sendable {
    public let pixelBuffer: CVPixelBuffer
    public let presentationTime: Double
    public let duration: Double
    public let isKeyframe: Bool
    public let width: Int
    public let height: Int
    public let frameNumber: Int64
}
```

### Fields

| Field | Description |
|-------|-------------|
| `pixelBuffer` | Decoded frame as CVPixelBuffer |
| `presentationTime` | PTS in seconds |
| `duration` | Frame duration in seconds |
| `isKeyframe` | True if I-frame |
| `width` / `height` | Frame dimensions |
| `frameNumber` | Sequential frame number |

### Frame Usage

```swift
// Get frame at specific time
if let frame = try decoder.getFrame(at: 5.0, tolerance: 0.016) {
    // Render to Metal
    metalView.render(pixelBuffer: frame.pixelBuffer)

    if frame.isKeyframe {
        print("Keyframe at \(frame.presentationTime)s")
    }
}
```

## FFmpegDecoder

Main decoder class:

```swift
public final class FFmpegDecoder: @unchecked Sendable {
    // Initialization
    public init(url: URL, configuration: DecoderConfiguration = .default) throws

    // Lifecycle
    public func prepare() async throws
    public func invalidate()

    // Decoding
    public func startDecoding()
    public func stopDecoding()
    public func seek(to time: Double) throws -> FFmpegFrame?

    // Frame access
    public func getFrame(at time: Double, tolerance: Double) throws -> FFmpegFrame?
    public func getNextFrame() -> FFmpegFrame?

    // Prefetch (for scrubbing)
    public func startPrefetch(direction: Int, velocity: Double)
    public func stopPrefetch()

    // Properties
    public var mediaInfo: FFmpegMediaInfo { get }
    public var cacheStatistics: CacheStatistics { get }
    public var currentTime: Double { get }
}
```

### Decoder Lifecycle

```swift
// 1. Create decoder
let decoder = try FFmpegDecoder(url: fileURL)

// 2. Prepare (async, loads metadata)
try await decoder.prepare()

// 3. Use decoder
let frame = try decoder.getFrame(at: 0.0, tolerance: 0.016)

// 4. Cleanup
decoder.invalidate()
```

## DecoderConfiguration

Decoder configuration options:

```swift
public struct DecoderConfiguration: Sendable {
    public let preferHardwareDecoding: Bool
    public let cacheConfiguration: CacheConfiguration
    public let threadCount: Int
    public let outputPixelFormat: PixelFormat

    public static let `default`: DecoderConfiguration
    public static let performance: DecoderConfiguration
    public static let lowMemory: DecoderConfiguration
}

public struct CacheConfiguration: Sendable {
    public let l1Capacity: Int      // Default: 30
    public let l2Capacity: Int      // Default: 100
    public let l3Capacity: Int      // Default: 500
    public let enablePrefetch: Bool // Default: true
}

public enum PixelFormat: String, Sendable {
    case bgra       // For Metal rendering
    case nv12       // VideoToolbox native
    case yuv420p    // Software decode
}
```

### Configuration Examples

```swift
// Default configuration
let decoder = try FFmpegDecoder(url: url)

// Performance configuration
let config = DecoderConfiguration.performance
let decoder = try FFmpegDecoder(url: url, configuration: config)

// Custom configuration
let config = DecoderConfiguration(
    preferHardwareDecoding: true,
    cacheConfiguration: CacheConfiguration(
        l1Capacity: 60,
        l2Capacity: 200,
        l3Capacity: 1000,
        enablePrefetch: true
    ),
    threadCount: 4,
    outputPixelFormat: .bgra
)
```

## CacheStatistics

Cache performance statistics:

```swift
public struct CacheStatistics: Sendable {
    public let l1Entries: Int
    public let l2Entries: Int
    public let l3Entries: Int
    public let totalEntries: Int

    public let l1HitCount: Int
    public let l2HitCount: Int
    public let l3HitCount: Int
    public let missCount: Int

    public let hitRate: Double
    public let memoryUsageBytes: Int
}
```

### Statistics Usage

```swift
let stats = decoder.cacheStatistics

print("Cache entries: \(stats.totalEntries)")
print("Hit rate: \(String(format: "%.1f", stats.hitRate * 100))%")
print("Memory: \(stats.memoryUsageBytes / 1024 / 1024) MB")
```

## FFmpegError

Error types:

```swift
public enum FFmpegError: Error, Sendable {
    case fileNotFound(URL)
    case invalidFormat(String)
    case codecNotSupported(String)
    case decodeFailed(String)
    case seekFailed(Double)
    case memoryError
    case rustPanic(String)
    case unknown(Int32)
}
```

### Error Handling

```swift
do {
    let decoder = try FFmpegDecoder(url: url)
    try await decoder.prepare()
} catch FFmpegError.codecNotSupported(let codec) {
    print("Codec \(codec) not supported")
} catch FFmpegError.fileNotFound(let url) {
    print("File not found: \(url)")
} catch {
    print("Unknown error: \(error)")
}
```

## FFmpegFrameProvider

Protocol for frame access:

```swift
public protocol FFmpegFrameProvider: Sendable {
    var mediaInfo: FFmpegMediaInfo { get }
    var currentTime: Double { get }

    func getFrame(at time: Double, tolerance: Double) throws -> FFmpegFrame?
    func getNextFrame() -> FFmpegFrame?
    func seek(to time: Double) throws -> FFmpegFrame?

    func startPrefetch(direction: Int, velocity: Double)
    func stopPrefetch()
}
```

### Protocol Conformance

`FFmpegDecoder` conforms to `FFmpegFrameProvider`, allowing abstraction:

```swift
func renderFrame(from provider: FFmpegFrameProvider, at time: Double) {
    if let frame = try? provider.getFrame(at: time, tolerance: 0.016) {
        metalView.render(pixelBuffer: frame.pixelBuffer)
    }
}
```
