# CYBFFmpeg

FFmpeg-based video decoding library for macOS, designed as a standalone Swift Package.

## Overview

CYBFFmpeg provides video decoding support for codecs not available in AVFoundation, including:

- VP8/VP9 (WebM)
- AV1
- MPEG-1/2
- WMV/VC-1
- DivX/XviD
- DNxHD/HR
- And more...

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│              CYBFFmpeg (Swift Package)                       │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Swift Public API                                        │ │
│  │  - FFmpegMediaInfo (media information)                  │ │
│  │  - FFmpegDecoder (decoder class)                        │ │
│  │  - FFmpegFrame (frame data)                             │ │
│  └────────────────────────────────────────────────────────┘ │
│                         │                                    │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Rust Core (cyb-ffmpeg-core)                            │ │
│  │  - ffmpeg-next bindings                                 │ │
│  │  - Multi-tier frame cache (L1/L2/L3)                   │ │
│  │  - VideoToolbox hardware acceleration                   │ │
│  └────────────────────────────────────────────────────────┘ │
│                         │                                    │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ FFmpeg Libraries (LGPL v3.0)                           │ │
│  │  - libavcodec, libavformat, libavutil, libswscale      │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Requirements

- macOS 14.0+
- Xcode 15.0+
- Rust 1.70+
- Build tools: `pkg-config`, `nasm` (via Homebrew)

The package builds FFmpeg 7.1.x from source as part of `./build-all.sh`. The
produced dylibs are LGPL v3.0 only (no GPL components) and self-contained
(no `/opt/homebrew/...` absolute path dependencies) so they can be embedded
in a sandboxed Mac App Store app and re-signed with the consumer app's
Team ID for library validation compliance.

External codec libraries (libvpx, libdav1d, …) are intentionally **not**
linked. VP9 and AV1 fall back to VideoToolbox hardware decode on Apple
Silicon — see `ffmpeg-build/scripts/build-ffmpeg.sh` for the codec matrix.

## Installation

### Prerequisites

1. Install build tools:
```bash
brew install pkg-config nasm
```

2. Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Building the binary artifacts (first run: 30–60 minutes)

`libcyb_ffmpeg_core.a` (Rust) and the FFmpeg `lib*.dylib` are gitignored.
After cloning, run the top-level orchestrator once to produce them:

```bash
./build-all.sh           # release build of both Rust core and FFmpeg
./build-all.sh --clean   # clean rebuild
./build-all.sh --debug   # debug build
./build-all.sh --skip-ffmpeg   # rebuild only the Rust core
```

The script verifies that produced dylibs have no external path dependencies
(Homebrew / /usr/local / /opt) and that GPL / nonfree codecs are disabled.
Subsequent runs (no `--clean`) take a few seconds because the FFmpeg source
tarball stays cached under `ffmpeg-build/build/`.

### Adding to Your Project

Add CYBFFmpeg as a local package dependency:

```swift
dependencies: [
    .package(path: "../CYBFFmpeg"),
]
```

## Usage

```swift
import CYBFFmpeg

// Create decoder
let decoder = try FFmpegDecoder(url: videoURL, configuration: .default)
try await decoder.prepare()

// Get media info
let info = decoder.mediaInfo
print("Duration: \(info.duration)s")
print("Video: \(info.videoTracks.first?.description ?? "none")")

// Decode frames
decoder.startDecoding()
while let frame = decoder.getNextFrame() {
    // Use frame.pixelBuffer
}
decoder.stopDecoding()
```

## License

- CYBFFmpeg: LGPL-2.1-or-later
- FFmpeg: LGPL v3.0 (no GPL components)

## Version

0.1.0
