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
- FFmpeg 7.x or 8.x (via Homebrew)

## Installation

### Prerequisites

1. Install FFmpeg:
```bash
brew install ffmpeg
```

2. Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Building the Rust Core

```bash
cd cyb-ffmpeg-core
cargo build --release
```

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
