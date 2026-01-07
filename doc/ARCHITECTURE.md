# Architecture — CYBFFmpeg

## Purpose

CYBFFmpeg is an **independent** Swift package providing FFmpeg-based media decoding with Rust-powered high-performance frame caching.

It acts as a specialized decoder for:

- Codecs not supported by AVFoundation (VP9, AV1, MPEG-1/2, DivX, DNxHD, etc.)
- High-speed scrubbing with intelligent frame caching
- Frame-accurate seeking with keyframe index

## High-level Architecture

```text
CYBFFmpeg
├─ FFmpegMediaInfo     (media metadata)
├─ FFmpegDecoder       (decoder interface)
├─ FFmpegFrameProvider (frame access)
├─ FFmpegFrame         (decoded frame)
└─ CacheStatistics     (cache status)
```

## Layered Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                    Layer 1: Public API (Swift)               │
│                                                              │
│  FFmpegMediaInfo    FFmpegDecoder    FFmpegFrameProvider     │
│  FFmpegFrame        FFmpegError      DecoderConfiguration    │
│                                                              │
│  - User-facing interfaces                                    │
│  - DocC documented                                           │
│  - Stable, rarely changes                                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Layer 2: Bridge (Swift)                   │
│                                                              │
│  RustBridge         PixelBufferConverter                     │
│                                                              │
│  - FFI call abstraction                                      │
│  - Error conversion (CybResult → FFmpegError)                │
│  - Memory management (frame lifecycle)                       │
│  - Internal implementation details hidden                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Layer 3: FFI (C Header)                   │
│                                                              │
│  cyb_ffmpeg.h (cbindgen generated)                          │
│                                                              │
│  - Thin C function interface                                 │
│  - Opaque handle types                                       │
│  - Auto-generated from Rust                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Layer 4: Rust Core                        │
│                                                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐     │
│  │   ffi/   │  │ decoder/ │  │  cache/  │  │threading/│     │
│  │          │  │          │  │          │  │          │     │
│  │ exports  │  │ ffmpeg-  │  │ L1/L2/L3 │  │ parallel │     │
│  │ cbindgen │  │ next     │  │ multi-   │  │ decode   │     │
│  │          │  │ wrapper  │  │ tier     │  │          │     │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘     │
│                                                              │
│  - ffmpeg-next bindings                                      │
│  - VideoToolbox hardware acceleration                        │
│  - High-frequency changes                                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    FFmpeg Libraries (LGPL)                   │
│                                                              │
│  libavcodec    libavformat    libavutil    libswscale        │
│  libdav1d      libvpx         libaom                         │
│                                                              │
│  - Dynamic libraries (.dylib)                                │
│  - LGPL v3.0 compliant                                       │
│  - Replaceable for user modification                         │
└─────────────────────────────────────────────────────────────┘
```

## Layer Responsibilities

| Layer      | Responsibility  | Change Frequency | Testing           |
| ---------- | --------------- | ---------------- | ----------------- |
| Public API | User interfaces | Low              | E2E tests         |
| Bridge     | FFI abstraction | Medium           | Swift integration |
| FFI        | C function defs | Low              | cbindgen verify   |
| Rust Core  | Decode & cache  | High             | Rust unit tests   |

## Design Principles

### Independence

CYBFFmpeg knows nothing about:

- CYBMediaPlayer
- CYBMediaHolder
- Any UI framework

Dependency direction is strictly one-way:

```text
CYBMediaHolder → CYBFFmpeg (optional)
CYBMediaPlayer → CYBFFmpeg (optional)
```

### LGPL Compliance

- FFmpeg built as dynamic libraries
- No GPL components (libx264, libx265)
- User can replace FFmpeg libraries
- Source code available

### Hardware Acceleration

VideoToolbox is preferred when available:

```text
Codec → Check VideoToolbox → HW decode
                ↓ (unavailable)
        Software decode via FFmpeg
```

### Performance First

- Rust for CPU-intensive operations
- Multi-tier cache for scrubbing
- Parallel decoding threads
- Zero-copy where possible

## Module Structure

```text
Sources/CYBFFmpeg/
├── Public/
│   ├── FFmpegMediaInfo.swift      # Media metadata structure
│   ├── FFmpegDecoder.swift        # Main decoder class
│   ├── FFmpegFrameProvider.swift  # Frame access protocol
│   ├── FFmpegFrame.swift          # Decoded frame data
│   ├── FFmpegError.swift          # Error types
│   └── Configuration.swift        # Decoder configuration
├── Internal/
│   ├── RustBridge.swift           # FFI call wrapper
│   ├── PixelBufferConverter.swift # CVPixelBuffer conversion
│   └── HandleManager.swift        # Rust handle lifecycle
└── CybFFmpegC/
    ├── include/
    │   └── cyb_ffmpeg.h           # Generated C header
    └── module.modulemap
```

## Platform Constraints

- macOS 14.0+ (Sonoma)
- Apple Silicon optimized
- Swift 6.0 concurrency
- All public types are `Sendable`
- No global mutable state
