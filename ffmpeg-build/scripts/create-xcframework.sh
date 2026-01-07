#!/bin/bash
# =============================================================================
# CYBFFmpeg - XCFramework Creation Script
# =============================================================================
# Creates an XCFramework bundle from the built FFmpeg libraries for
# easy integration with Xcode projects and Swift packages.
#
# Usage:
#   ./create-xcframework.sh [--output path]
#
# =============================================================================

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_OUTPUT="${SCRIPT_DIR}/../output"
XCFRAMEWORK_OUTPUT="${SCRIPT_DIR}/../FFmpeg.xcframework"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --output)
            XCFRAMEWORK_OUTPUT="$2"
            shift 2
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check that build output exists
if [ ! -d "$BUILD_OUTPUT/lib" ]; then
    log_error "Build output not found at: $BUILD_OUTPUT/lib"
    log_error "Run build-ffmpeg.sh first"
    exit 1
fi

log_info "Creating XCFramework..."

# Remove existing XCFramework
if [ -d "$XCFRAMEWORK_OUTPUT" ]; then
    log_info "Removing existing XCFramework..."
    rm -rf "$XCFRAMEWORK_OUTPUT"
fi

# Create temporary framework directory
TEMP_DIR=$(mktemp -d)
FRAMEWORK_NAME="CYBFFmpegLibs"
FRAMEWORK_DIR="${TEMP_DIR}/${FRAMEWORK_NAME}.framework"

log_info "Creating framework structure..."

mkdir -p "${FRAMEWORK_DIR}/Headers"
mkdir -p "${FRAMEWORK_DIR}/Modules"

# Copy headers
cp -R "${BUILD_OUTPUT}/include/"* "${FRAMEWORK_DIR}/Headers/"

# Create umbrella header
cat > "${FRAMEWORK_DIR}/Headers/CYBFFmpegLibs.h" << 'EOF'
#ifndef CYBFFmpegLibs_h
#define CYBFFmpegLibs_h

// FFmpeg core headers
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libswscale/swscale.h>

#endif /* CYBFFmpegLibs_h */
EOF

# Create module map
cat > "${FRAMEWORK_DIR}/Modules/module.modulemap" << 'EOF'
framework module CYBFFmpegLibs {
    umbrella header "CYBFFmpegLibs.h"

    export *
    module * { export * }

    link "avcodec"
    link "avformat"
    link "avutil"
    link "swscale"
    link "swresample"
}
EOF

# Create Info.plist
CURRENT_VERSION="7.0.1"
cat > "${FRAMEWORK_DIR}/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${FRAMEWORK_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.cyberseeds.CYBFFmpegLibs</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${FRAMEWORK_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>${CURRENT_VERSION}</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>MinimumOSVersion</key>
    <string>14.0</string>
    <key>NSHumanReadableCopyright</key>
    <string>LGPL v3.0</string>
</dict>
</plist>
EOF

# Collect all dylibs
log_info "Collecting dynamic libraries..."

DYLIBS=(
    "${BUILD_OUTPUT}/lib/libavcodec.dylib"
    "${BUILD_OUTPUT}/lib/libavformat.dylib"
    "${BUILD_OUTPUT}/lib/libavutil.dylib"
    "${BUILD_OUTPUT}/lib/libswscale.dylib"
    "${BUILD_OUTPUT}/lib/libswresample.dylib"
)

# Create combined dylib (or use lipo for multi-arch)
log_info "Creating framework binary..."

# For simplicity, we'll copy the main library and set up symlinks
# In a real multi-arch build, you'd use lipo here

# Copy all dylibs to framework
cp "${BUILD_OUTPUT}/lib/"*.dylib "${FRAMEWORK_DIR}/"

# Create a "fake" framework binary that re-exports all libraries
# This is a common pattern for bundling multiple dylibs

# For now, we'll create the XCFramework with individual libraries
log_info "Creating XCFramework..."

# Detect architecture
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    PLATFORM="macos-arm64"
else
    PLATFORM="macos-x86_64"
fi

# Create XCFramework with the framework
xcodebuild -create-xcframework \
    -framework "${FRAMEWORK_DIR}" \
    -output "${XCFRAMEWORK_OUTPUT}"

# Also copy dylibs directly for projects that need them
DYLIB_OUTPUT="${SCRIPT_DIR}/../dylibs"
mkdir -p "${DYLIB_OUTPUT}"
cp "${BUILD_OUTPUT}/lib/"*.dylib "${DYLIB_OUTPUT}/"

log_info "Copied dylibs to: ${DYLIB_OUTPUT}"

# Cleanup
rm -rf "${TEMP_DIR}"

# Print summary
log_info "========================================"
log_info "XCFramework created successfully!"
log_info "========================================"
log_info "Output: ${XCFRAMEWORK_OUTPUT}"
log_info "Dylibs: ${DYLIB_OUTPUT}"
log_info ""
log_info "To use in your project:"
log_info "  1. Drag FFmpeg.xcframework to your Xcode project"
log_info "  2. Or add to Package.swift as a binary target"
log_info ""
log_info "Package.swift example:"
cat << 'EOF'
    .binaryTarget(
        name: "CYBFFmpegLibs",
        path: "ffmpeg-build/FFmpeg.xcframework"
    ),
EOF
log_info "========================================"
