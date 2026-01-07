# Claude.md — CYBFFmpeg

## What this repository is

CYBFFmpeg is a **completely independent** Swift package that provides FFmpeg-based media decoding with Rust-powered high-performance frame caching.

It is intentionally independent from CYBMediaPlayer, CYBMediaHolder, and any UI implementations.

This package outputs its own `FFmpegMediaInfo` structure, which can be normalized by CYBMediaHolder when needed.

---

## Non-negotiable principles

- CYBFFmpeg **does NOT know** CYBMediaPlayer, CYBMediaHolder, or any UI.
- CYBFFmpeg **does NOT depend** on any other CYB packages.
- LGPL v3.0 compliance is **mandatory** for Mac App Store distribution.
- Layered architecture must be maintained:
  - Public API (Swift) → Bridge (Swift) → FFI (C) → Rust Core
- All public types must be `Sendable`.
- Swift Concurrency is the default mental model.
- VideoToolbox/AudioToolbox hardware acceleration is preferred.

---

## Architectural boundaries (do not blur)

```
CYBFFmpeg/
├── Sources/CYBFFmpeg/
│   ├── Public/           # Public Swift API (user-facing)
│   │   ├── FFmpegMediaInfo.swift
│   │   ├── FFmpegDecoder.swift
│   │   ├── FFmpegFrameProvider.swift
│   │   └── FFmpegError.swift
│   ├── Internal/         # Internal implementation
│   │   ├── RustBridge.swift
│   │   └── PixelBufferConverter.swift
│   └── CybFFmpegC/       # C shim for Rust FFI
│       ├── include/
│       │   └── cyb_ffmpeg.h
│       └── module.modulemap
├── cyb-ffmpeg-core/      # Rust crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── ffi/          # FFI exports
│   │   ├── decoder/      # FFmpeg decoding
│   │   ├── cache/        # Multi-tier frame cache
│   │   └── threading/    # Parallel decoding
│   ├── Cargo.toml
│   └── cbindgen.toml
└── ffmpeg-build/         # FFmpeg LGPL build scripts
    └── scripts/
```

---

## Layer responsibilities

| Layer | Responsibility | Change Frequency |
|-------|---------------|------------------|
| **Public API** | User-facing interfaces, DocC documented | Low |
| **Bridge** | FFI abstraction, error conversion, memory management | Medium |
| **FFI** | Thin C function interface, cbindgen generated | Low |
| **Rust Core** | Decoding, caching, threading | High |

---

## Guardrails (do NOT do)

- Do NOT introduce playback control logic (play/pause/volume).
- Do NOT add dependencies to CYBMediaPlayer or CYBMediaHolder.
- Do NOT use GPL-licensed FFmpeg components (libx264, libx265, etc.).
- Do NOT bypass the layered architecture.
- Do NOT expose Rust types directly to Swift.
- Do NOT add global mutable state.
- Do NOT ignore VideoToolbox availability for hardware decoding.

---

## Cache System (Multi-tier)

Frame caching uses a 3-tier architecture for optimal scrubbing performance:

### Cache Tiers

| Tier | Capacity | Strategy | Purpose |
|------|----------|----------|---------|
| **L1 (Hot)** | 30 frames | LRU | Immediate access, current playhead |
| **L2 (Keyframe)** | 100 frames | Keyframe-only | Quick seeking anchors |
| **L3 (Cold)** | 500 frames | SIEVE eviction | Background prefetch |

### Cache Operations

```swift
// Prefetch for scrubbing
decoder.startPrefetch(direction: 1, velocity: 2.0)

// Get statistics
let stats = decoder.cacheStatistics
print("Hit rate: \(stats.hitRate)%")
```

### Performance Targets

- Scrub response: <100ms
- Frame decode: <16ms (60fps capable)
- Cache hit rate: >80%

---

## LGPL Compliance (Mac App Store)

### Required Configuration

FFmpeg must be built with:
```bash
--enable-shared --disable-static
--enable-version3
--disable-gpl --disable-nonfree
```

### Allowed Components

| Component | License | Status |
|-----------|---------|--------|
| libavcodec | LGPL | OK |
| libavformat | LGPL | OK |
| libavutil | LGPL | OK |
| libdav1d | BSD-2 | OK |
| libvpx | BSD | OK |
| libaom | BSD-2 | OK |
| VideoToolbox | System | OK |

### Prohibited Components

| Component | License | Status |
|-----------|---------|--------|
| libx264 | GPL | PROHIBITED |
| libx265 | GPL | PROHIBITED |
| libfdk-aac | Non-free | PROHIBITED |

### Verification

Always run before release:
```bash
./ffmpeg-build/scripts/verify-lgpl.sh
```

---

## Codec Support

### Hardware Accelerated (VideoToolbox)

- H.264/AVC
- H.265/HEVC
- VP9 (macOS 11+)
- ProRes

### Software Decoding

| Codec | Library | Use Case |
|-------|---------|----------|
| VP9 | libvpx | WebM, YouTube |
| AV1 | libdav1d | Modern streaming |
| MPEG-1/2 | Native | Legacy content |
| MPEG-4 | Native | DivX, Xvid |
| DNxHD/HR | Native | Professional |

---

## Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    External Usage                            │
│                                                              │
│  CYBMediaHolder (optional)                                   │
│       │                                                      │
│       ▼                                                      │
│  FFmpegMediaInfo → MediaDescriptor (normalization)           │
│                                                              │
│  CYBMediaPlayer                                              │
│       │                                                      │
│       ▼                                                      │
│  FFmpegDecoder → CVPixelBuffer → MetalView                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    CYBFFmpeg Internal                        │
│                                                              │
│  Swift Public API                                            │
│       │                                                      │
│       ▼                                                      │
│  RustBridge (FFI calls)                                      │
│       │                                                      │
│       ▼                                                      │
│  Rust Core (ffmpeg-next, cache, threading)                   │
│       │                                                      │
│       ▼                                                      │
│  FFmpeg Libraries (LGPL)                                     │
└─────────────────────────────────────────────────────────────┘
```

---

## Canonical documentation (single source of truth)

- Architecture overview → `doc/ARCHITECTURE.md`
- Public API usage → `doc/API_GUIDE.md`
- Core types → `doc/CORE_TYPES.md`
- Cache system → `doc/CACHE.md`
- Rust FFI → `doc/RUST_FFI.md`
- FFmpeg build → `doc/FFMPEG_BUILD.md`
- LGPL compliance → `doc/LGPL_COMPLIANCE.md`
- Testing → `doc/TESTING.md`
- Roadmap → `doc/ROADMAP.md`

---

## How AI should work in this repo

- Treat `doc/` as canonical knowledge.
- Always verify LGPL compliance before adding FFmpeg components.
- For Rust changes, ensure cbindgen regenerates headers.
- Test cache performance after modifying cache layer.
- Prefer documenting decisions over clever implementations.
- For large refactors, ask for confirmation first.

---

## When in doubt

Stop and ask.
Preserve architectural intent over local optimization.
Never compromise LGPL compliance.
