# Roadmap — CYBFFmpeg

## Current Status: Phase 1 (Foundation)

### Completed

- [x] Package structure design
- [x] CLAUDE.md documentation
- [x] doc/ folder with specifications
- [x] Architecture documentation
- [x] LGPL compliance guidelines

### In Progress

- [ ] Package.swift configuration
- [ ] Directory structure creation
- [ ] C header skeleton
- [ ] Module.modulemap

## Phase 1: Package Foundation

**Goal**: Swift Package skeleton with FFI structure

### Tasks

- [ ] Create `Sources/CYBFFmpeg/Public/` directory
- [ ] Create `Sources/CYBFFmpeg/Internal/` directory
- [ ] Create `Sources/CYBFFmpeg/CybFFmpegC/include/` directory
- [ ] Create module.modulemap for C interop
- [ ] Create placeholder cyb_ffmpeg.h
- [ ] Create placeholder Swift files
- [ ] Verify package builds (empty)

### Deliverables

```
CYBFFmpeg/
├── Package.swift
├── CLAUDE.md
├── doc/
├── Sources/
│   └── CYBFFmpeg/
│       ├── Public/
│       ├── Internal/
│       └── CybFFmpegC/
│           ├── include/cyb_ffmpeg.h
│           └── module.modulemap
└── Tests/
    └── CYBFFmpegTests/
```

## Phase 2: Public API Design

**Goal**: Define stable Swift interfaces

### Tasks

- [ ] `FFmpegMediaInfo.swift` - Media information structure
- [ ] `FFmpegDecoder.swift` - Main decoder class
- [ ] `FFmpegFrame.swift` - Frame data structure
- [ ] `FFmpegFrameProvider.swift` - Frame access protocol
- [ ] `FFmpegError.swift` - Error types
- [ ] `Configuration.swift` - Decoder configuration

### API Surface

```swift
// Key types
public struct FFmpegMediaInfo: Sendable
public struct FFmpegVideoTrack: Sendable
public struct FFmpegAudioTrack: Sendable
public struct FFmpegCodec: Sendable
public struct FFmpegFrame: @unchecked Sendable
public struct DecoderConfiguration: Sendable
public struct CacheConfiguration: Sendable
public struct CacheStatistics: Sendable
public enum FFmpegError: Error, Sendable

// Key protocols
public protocol FFmpegFrameProvider: Sendable

// Main class
public final class FFmpegDecoder: @unchecked Sendable
```

## Phase 3: Rust Core Implementation

**Goal**: Implement decoding and caching in Rust

### Tasks

- [ ] `cyb-ffmpeg-core/Cargo.toml`
- [ ] `cyb-ffmpeg-core/cbindgen.toml`
- [ ] `src/lib.rs` - Crate entry point
- [ ] `src/ffi/mod.rs` - FFI exports
- [ ] `src/decoder/mod.rs` - FFmpeg wrapper
- [ ] `src/cache/mod.rs` - Multi-tier cache
- [ ] `src/threading/mod.rs` - Parallel decode

### Rust Modules

```rust
// lib.rs
mod ffi;
mod decoder;
mod cache;
mod threading;
mod error;

// Key exports via FFI
cyb_decoder_create()
cyb_decoder_prepare()
cyb_decoder_destroy()
cyb_decoder_get_frame_at()
cyb_decoder_start_prefetch()
cyb_decoder_stop_prefetch()
cyb_decoder_get_cache_stats()
```

### Dependencies

```toml
[dependencies]
ffmpeg-next = "7.0"
crossbeam = "0.8"
parking_lot = "0.12"
thiserror = "1.0"
```

## Phase 4: FFmpeg Build System

**Goal**: LGPL-compliant FFmpeg build

### Tasks

- [ ] `ffmpeg-build/scripts/build-ffmpeg.sh`
- [ ] `ffmpeg-build/scripts/verify-lgpl.sh`
- [ ] `ffmpeg-build/scripts/create-xcframework.sh`
- [ ] CI/CD integration
- [ ] Test on Apple Silicon

### Build Targets

- FFmpeg 7.0
- macOS 14.0+ / arm64
- LGPL v3.0 compliant
- VideoToolbox enabled
- libdav1d, libvpx, libaom

## Phase 5: CYBMediaHolder Integration

**Goal**: Optional normalization adapter

### Tasks

- [ ] `FFmpegAdapter.swift` in CYBMediaHolder
- [ ] `FFmpegMediaInfo` → `MediaDescriptor` conversion
- [ ] Optional dependency configuration
- [ ] Integration tests

### API

```swift
// In CYBMediaHolder
extension FFmpegMediaInfo {
    func toMediaDescriptor() -> MediaDescriptor
}
```

## Phase 6: CYBMediaPlayer Integration

**Goal**: Use CYBFFmpeg in player

### Tasks

- [ ] Add CYBFFmpeg dependency to Package.swift
- [ ] `FFmpegDecoderBridge.swift` in MediaPlayerEngine
- [ ] Fallback logic (AVFoundation → FFmpeg)
- [ ] MetalView integration
- [ ] Integration tests

### Integration Pattern

```swift
class MediaPlayerEngine {
    private var avDecoder: AVFoundationDecoder?
    private var ffmpegDecoder: FFmpegDecoder?

    func load(url: URL) async throws {
        if canUseAVFoundation(url) {
            avDecoder = try await AVFoundationDecoder(url: url)
        } else {
            ffmpegDecoder = try FFmpegDecoder(url: url)
            try await ffmpegDecoder?.prepare()
        }
    }
}
```

## Phase 7: Testing & Optimization

**Goal**: Production-ready quality

### Tasks

- [ ] Swift E2E tests
- [ ] Rust unit tests
- [ ] Performance benchmarks
- [ ] Memory profiling
- [ ] LGPL compliance verification
- [ ] Documentation review

### Performance Targets

| Metric | Target |
|--------|--------|
| Frame decode (1080p) | <16ms |
| Frame decode (4K) | <33ms |
| Scrub response | <100ms |
| Cache hit rate | >80% |
| Memory (1080p) | <500MB |

## Future Enhancements

### Post-MVP Features

- [ ] Audio decoding support
- [ ] Subtitle extraction
- [ ] Frame export (still images)
- [ ] Hardware encoding
- [ ] Network streaming support

### Codec Expansion

- [ ] MPEG-TS container
- [ ] MKV metadata
- [ ] HDR10+ metadata
- [ ] Dolby Vision profile parsing

### Platform Expansion

- [ ] iOS support (limited FFmpeg)
- [ ] visionOS exploration
- [ ] Framework distribution (SPM, CocoaPods)

## Version History

### v0.1.0 (Planned)

- Initial implementation
- VP9, AV1, MPEG-1/2/4 support
- Multi-tier frame cache
- Scrubbing optimization

### v0.2.0 (Future)

- Audio decoding
- Improved cache strategies
- Performance optimizations

### v1.0.0 (Future)

- Production-ready release
- Complete documentation
- Mac App Store approved
