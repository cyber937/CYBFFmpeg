# FFmpeg Build — CYBFFmpeg

This document describes how to build FFmpeg for CYBFFmpeg with LGPL compliance.

## Requirements

### Build Dependencies

```bash
# Homebrew packages
brew install nasm yasm pkg-config automake libtool

# Codec libraries
brew install dav1d libvpx aom
```

### macOS SDK

- Xcode 15.0 or later
- macOS 14.0 SDK
- Apple Silicon support (arm64)

## Build Configuration

### LGPL Compliance (REQUIRED)

```bash
./configure \
  --prefix=/usr/local/ffmpeg-lgpl \
  --enable-shared \
  --disable-static \
  --enable-version3 \
  --disable-gpl \
  --disable-nonfree \
  --disable-programs \
  --disable-doc
```

### Hardware Acceleration

```bash
# VideoToolbox (Apple Silicon)
--enable-videotoolbox \
--enable-audiotoolbox \
--enable-hwaccel=h264_videotoolbox \
--enable-hwaccel=hevc_videotoolbox \
--enable-hwaccel=vp9_videotoolbox \
--enable-hwaccel=prores_videotoolbox
```

### Codec Libraries

```bash
# LGPL-compatible external libraries
--enable-libdav1d \      # AV1 decoder (BSD-2-Clause)
--enable-libvpx \        # VP8/VP9 (BSD)
--enable-libaom          # AV1 encoder/decoder (BSD-2-Clause)
```

### PROHIBITED Options

```bash
# DO NOT USE - These violate LGPL
--enable-gpl             # PROHIBITED
--enable-nonfree         # PROHIBITED
--enable-libx264         # PROHIBITED (GPL)
--enable-libx265         # PROHIBITED (GPL)
--enable-libfdk-aac      # PROHIBITED (Non-free)
```

## Complete Build Script

### build-ffmpeg.sh

```bash
#!/bin/bash
set -e

FFMPEG_VERSION="7.0"
PREFIX="$(pwd)/output/ffmpeg"
DEPLOYMENT_TARGET="14.0"

# Download FFmpeg
if [ ! -d "ffmpeg-${FFMPEG_VERSION}" ]; then
    curl -LO "https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
    tar xf "ffmpeg-${FFMPEG_VERSION}.tar.xz"
fi

cd "ffmpeg-${FFMPEG_VERSION}"

# Configure for arm64 (Apple Silicon)
./configure \
  --prefix="${PREFIX}" \
  --enable-shared \
  --disable-static \
  --enable-version3 \
  --disable-gpl \
  --disable-nonfree \
  --disable-programs \
  --disable-doc \
  --enable-videotoolbox \
  --enable-audiotoolbox \
  --enable-hwaccel=h264_videotoolbox \
  --enable-hwaccel=hevc_videotoolbox \
  --enable-hwaccel=vp9_videotoolbox \
  --enable-hwaccel=prores_videotoolbox \
  --enable-libdav1d \
  --enable-libvpx \
  --enable-libaom \
  --arch=arm64 \
  --target-os=darwin \
  --extra-cflags="-mmacosx-version-min=${DEPLOYMENT_TARGET}" \
  --extra-ldflags="-mmacosx-version-min=${DEPLOYMENT_TARGET}"

# Build
make -j$(sysctl -n hw.ncpu)
make install

echo "FFmpeg built successfully to ${PREFIX}"
```

## LGPL Verification

### verify-lgpl.sh

```bash
#!/bin/bash
set -e

FFMPEG_BIN="${1:-$(which ffmpeg)}"
FFMPEG_DIR="$(dirname "$FFMPEG_BIN")/../lib"

echo "Verifying LGPL compliance..."
echo "FFmpeg libraries: ${FFMPEG_DIR}"
echo ""

# Check for GPL markers
GPL_FOUND=false

for lib in "${FFMPEG_DIR}"/*.dylib; do
    if [ -f "$lib" ]; then
        # Check library symbols
        if nm "$lib" 2>/dev/null | grep -q "x264\|x265\|openh264"; then
            echo "ERROR: GPL codec found in $(basename "$lib")"
            GPL_FOUND=true
        fi
    fi
done

# Check FFmpeg configuration
if [ -f "${FFMPEG_DIR}/../include/libavutil/ffversion.h" ]; then
    if grep -q "enable-gpl\|enable-nonfree" "${FFMPEG_DIR}/../include/libavutil/ffversion.h"; then
        echo "ERROR: GPL/Non-free flags detected in build configuration"
        GPL_FOUND=true
    fi
fi

# Check ffmpeg binary (if exists)
if [ -x "$FFMPEG_BIN" ]; then
    CONFIG=$("$FFMPEG_BIN" -version 2>&1 | head -20)
    if echo "$CONFIG" | grep -q "enable-gpl"; then
        echo "ERROR: FFmpeg binary has --enable-gpl"
        GPL_FOUND=true
    fi
    if echo "$CONFIG" | grep -q "enable-nonfree"; then
        echo "ERROR: FFmpeg binary has --enable-nonfree"
        GPL_FOUND=true
    fi
    if echo "$CONFIG" | grep -q "libx264\|libx265\|libfdk"; then
        echo "ERROR: GPL/Non-free library linked"
        GPL_FOUND=true
    fi
fi

echo ""
if [ "$GPL_FOUND" = true ]; then
    echo "VERIFICATION FAILED: GPL components detected"
    exit 1
else
    echo "VERIFICATION PASSED: No GPL components found"
    echo "This build is LGPL-compliant for Mac App Store distribution"
fi
```

## XCFramework Creation

### create-xcframework.sh

```bash
#!/bin/bash
set -e

OUTPUT_DIR="$(pwd)/output"
FRAMEWORK_NAME="FFmpeg"

# Create framework structure
create_framework() {
    local ARCH=$1
    local LIB_DIR=$2
    local FW_DIR="${OUTPUT_DIR}/${ARCH}/${FRAMEWORK_NAME}.framework"

    mkdir -p "${FW_DIR}/Headers"
    mkdir -p "${FW_DIR}/Modules"

    # Copy headers
    cp -R "${LIB_DIR}/include/"* "${FW_DIR}/Headers/"

    # Create umbrella header
    cat > "${FW_DIR}/Headers/FFmpeg.h" << 'EOF'
#ifndef FFmpeg_h
#define FFmpeg_h

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libswscale/swscale.h>

#endif /* FFmpeg_h */
EOF

    # Create module map
    cat > "${FW_DIR}/Modules/module.modulemap" << 'EOF'
framework module FFmpeg {
    umbrella header "FFmpeg.h"
    export *
    module * { export * }
}
EOF

    # Merge libraries into single dylib
    local LIBS=$(find "${LIB_DIR}/lib" -name "*.dylib" -not -name "*.*.dylib")
    lipo -create ${LIBS} -output "${FW_DIR}/${FRAMEWORK_NAME}"

    # Create Info.plist
    cat > "${FW_DIR}/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.cyberseeds.FFmpeg</string>
    <key>CFBundleName</key>
    <string>FFmpeg</string>
    <key>CFBundleVersion</key>
    <string>7.0</string>
    <key>CFBundleShortVersionString</key>
    <string>7.0</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
</dict>
</plist>
EOF
}

# Create frameworks for each architecture
create_framework "arm64" "${OUTPUT_DIR}/ffmpeg"

# Create XCFramework
xcodebuild -create-xcframework \
    -framework "${OUTPUT_DIR}/arm64/${FRAMEWORK_NAME}.framework" \
    -output "${OUTPUT_DIR}/${FRAMEWORK_NAME}.xcframework"

echo "XCFramework created: ${OUTPUT_DIR}/${FRAMEWORK_NAME}.xcframework"
```

## Directory Structure

After build:

```
CYBFFmpeg/ffmpeg-build/
├── scripts/
│   ├── build-ffmpeg.sh
│   ├── verify-lgpl.sh
│   └── create-xcframework.sh
└── output/
    ├── ffmpeg/
    │   ├── bin/
    │   ├── include/
    │   │   ├── libavcodec/
    │   │   ├── libavformat/
    │   │   ├── libavutil/
    │   │   └── libswscale/
    │   └── lib/
    │       ├── libavcodec.dylib
    │       ├── libavformat.dylib
    │       ├── libavutil.dylib
    │       └── libswscale.dylib
    └── FFmpeg.xcframework/
        └── macos-arm64/
            └── FFmpeg.framework/
```

## Library Dependencies

### Native Codecs (No External Library)

| Codec | Type | Notes |
|-------|------|-------|
| MPEG-1/2 Video | Decoder | Legacy |
| MPEG-4 Part 2 | Decoder | DivX, Xvid |
| MJPEG | Decoder | Motion JPEG |
| DNxHD/DNxHR | Decoder | Avid professional |
| AAC | Decoder | Native (not libfdk) |
| MP3 | Decoder | MPEG Audio Layer 3 |
| FLAC | Decoder | Lossless audio |

### VideoToolbox Accelerated

| Codec | Hardware | Fallback |
|-------|----------|----------|
| H.264/AVC | Yes | Software |
| H.265/HEVC | Yes | Software |
| VP9 | macOS 11+ | libvpx |
| ProRes | Yes | Software |

### External Libraries (LGPL/BSD)

| Library | Codec | License |
|---------|-------|---------|
| libdav1d | AV1 | BSD-2-Clause |
| libvpx | VP8/VP9 | BSD |
| libaom | AV1 | BSD-2-Clause |

## Rust Integration

### Cargo.toml Configuration

```toml
[build-dependencies]
pkg-config = "0.3"

[dependencies]
ffmpeg-next = { version = "7.0", default-features = false }
```

### build.rs

```rust
use std::env;

fn main() {
    // Use pkg-config to find FFmpeg
    let ffmpeg_dir = env::var("FFMPEG_DIR")
        .unwrap_or_else(|_| "/usr/local/ffmpeg-lgpl".to_string());

    println!("cargo:rustc-link-search=native={}/lib", ffmpeg_dir);
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avformat");
    println!("cargo:rustc-link-lib=avutil");
    println!("cargo:rustc-link-lib=swscale");

    // VideoToolbox frameworks
    println!("cargo:rustc-link-lib=framework=VideoToolbox");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-lib=framework=CoreVideo");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
}
```

## Troubleshooting

### Common Issues

**1. Missing dav1d/vpx**
```bash
brew install dav1d libvpx aom
export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:$PKG_CONFIG_PATH"
```

**2. VideoToolbox not found**
```bash
# Ensure Xcode Command Line Tools are installed
xcode-select --install
```

**3. Linking errors**
```bash
# Set library path
export DYLD_LIBRARY_PATH="${PWD}/output/ffmpeg/lib:$DYLD_LIBRARY_PATH"
```

**4. Verification fails**
```bash
# Rebuild without GPL components
make clean
./configure ... --disable-gpl --disable-nonfree
make -j$(sysctl -n hw.ncpu)
```

## CI Integration

### GitHub Actions Example

```yaml
jobs:
  build-ffmpeg:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4

      - name: Install dependencies
        run: |
          brew install nasm yasm pkg-config dav1d libvpx aom

      - name: Build FFmpeg
        run: |
          cd CYBFFmpeg/ffmpeg-build/scripts
          ./build-ffmpeg.sh

      - name: Verify LGPL
        run: |
          cd CYBFFmpeg/ffmpeg-build/scripts
          ./verify-lgpl.sh ../output/ffmpeg/bin/ffmpeg
```
