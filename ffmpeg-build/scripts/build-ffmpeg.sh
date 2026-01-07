#!/bin/bash
# =============================================================================
# CYBFFmpeg - LGPL v3.0 Compliant FFmpeg Build Script
# =============================================================================
# This script builds FFmpeg with LGPL-only components for Mac App Store
# distribution. NO GPL components (libx264, libx265, etc.) are included.
#
# Supported codecs:
# - VP9 (libvpx) - BSD license
# - AV1 (libdav1d) - BSD-2-Clause license
# - MPEG-1/2/4 - Native LGPL
# - DNxHD/HR - Native LGPL
# - H.264/HEVC - VideoToolbox (Apple system framework)
#
# Usage:
#   ./build-ffmpeg.sh [--clean] [--debug]
#
# =============================================================================

set -e

# Configuration
FFMPEG_VERSION="7.0.1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/../build"
OUTPUT_DIR="${SCRIPT_DIR}/../output"
SOURCE_DIR="${BUILD_DIR}/ffmpeg-${FFMPEG_VERSION}"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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
CLEAN_BUILD=false
DEBUG_BUILD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --clean)
            CLEAN_BUILD=true
            shift
            ;;
        --debug)
            DEBUG_BUILD=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Clean if requested
if [ "$CLEAN_BUILD" = true ]; then
    log_info "Cleaning build directory..."
    rm -rf "${BUILD_DIR}"
    rm -rf "${OUTPUT_DIR}"
fi

# Create directories
mkdir -p "${BUILD_DIR}"
mkdir -p "${OUTPUT_DIR}/lib"
mkdir -p "${OUTPUT_DIR}/include"

# Detect architecture
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    TARGET_ARCH="arm64"
    log_info "Building for Apple Silicon (arm64)"
else
    TARGET_ARCH="x86_64"
    log_info "Building for Intel (x86_64)"
fi

# Check for Homebrew dependencies
check_dependencies() {
    log_info "Checking dependencies..."

    local missing_deps=()

    # Required tools
    for tool in pkg-config nasm; do
        if ! command -v "$tool" &> /dev/null; then
            missing_deps+=("$tool")
        fi
    done

    # LGPL-safe codec libraries
    for lib in libvpx dav1d; do
        if ! pkg-config --exists "$lib" 2>/dev/null; then
            missing_deps+=("$lib")
        fi
    done

    if [ ${#missing_deps[@]} -gt 0 ]; then
        log_warn "Missing dependencies: ${missing_deps[*]}"
        log_info "Installing via Homebrew..."
        brew install "${missing_deps[@]}"
    fi

    log_info "All dependencies satisfied"
}

# Download FFmpeg source
download_ffmpeg() {
    if [ -d "${SOURCE_DIR}" ]; then
        log_info "FFmpeg source already exists"
        return
    fi

    log_info "Downloading FFmpeg ${FFMPEG_VERSION}..."
    cd "${BUILD_DIR}"

    curl -LO "https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
    tar xf "ffmpeg-${FFMPEG_VERSION}.tar.xz"
    rm "ffmpeg-${FFMPEG_VERSION}.tar.xz"

    log_info "FFmpeg source downloaded"
}

# Configure FFmpeg with LGPL-only options
configure_ffmpeg() {
    log_info "Configuring FFmpeg (LGPL v3.0 only)..."
    cd "${SOURCE_DIR}"

    # Base configure options
    local CONFIGURE_OPTIONS=(
        --prefix="${OUTPUT_DIR}"

        # License: LGPL v3.0 ONLY
        --enable-shared
        --disable-static
        --enable-version3

        # CRITICAL: Disable GPL components
        --disable-gpl
        --disable-nonfree

        # Disable GPL libraries (MUST NOT be included for App Store)
        --disable-libx264
        --disable-libx265
        --disable-libxvid
        --disable-libfdk-aac

        # Enable LGPL-safe external libraries
        --enable-libvpx          # VP9 - BSD license
        --enable-libdav1d        # AV1 - BSD-2-Clause

        # Enable Apple hardware acceleration (system frameworks = allowed)
        --enable-videotoolbox
        --enable-audiotoolbox

        # Optimize for target architecture
        --arch="${TARGET_ARCH}"

        # macOS specific
        --enable-cross-compile
        --target-os=darwin

        # Disable unnecessary components
        --disable-programs       # No ffmpeg/ffprobe binaries
        --disable-doc
        --disable-htmlpages
        --disable-manpages
        --disable-podpages
        --disable-txtpages

        # Disable network (not needed for local file playback)
        --disable-network

        # Disable protocols we don't need
        --disable-protocols
        --enable-protocol=file
        --enable-protocol=pipe

        # Disable devices
        --disable-devices

        # Enable demuxers (containers)
        --enable-demuxer=mov
        --enable-demuxer=matroska
        --enable-demuxer=webm
        --enable-demuxer=mp4
        --enable-demuxer=avi
        --enable-demuxer=mpegts
        --enable-demuxer=mpegps
        --enable-demuxer=mxf

        # Enable parsers
        --enable-parser=h264
        --enable-parser=hevc
        --enable-parser=vp9
        --enable-parser=av1
        --enable-parser=mpeg4video
        --enable-parser=mpegvideo

        # Enable LGPL decoders
        --enable-decoder=h264
        --enable-decoder=hevc
        --enable-decoder=vp9
        --enable-decoder=av1
        --enable-decoder=libvpx_vp9
        --enable-decoder=libdav1d
        --enable-decoder=mpeg1video
        --enable-decoder=mpeg2video
        --enable-decoder=mpeg4
        --enable-decoder=prores
        --enable-decoder=dnxhd
        --enable-decoder=rawvideo

        # Audio decoders (all LGPL)
        --enable-decoder=aac
        --enable-decoder=mp3
        --enable-decoder=flac
        --enable-decoder=pcm_s16le
        --enable-decoder=pcm_s24le
        --enable-decoder=pcm_s32le
        --enable-decoder=pcm_f32le

        # Hardware-accelerated decoders (VideoToolbox)
        --enable-decoder=h264_videotoolbox
        --enable-decoder=hevc_videotoolbox
        --enable-decoder=vp9_videotoolbox
        --enable-decoder=prores_videotoolbox

        # Install name for dylib
        --install-name-dir="@rpath"
    )

    # Debug options
    if [ "$DEBUG_BUILD" = true ]; then
        CONFIGURE_OPTIONS+=(
            --enable-debug
            --disable-optimizations
        )
    else
        CONFIGURE_OPTIONS+=(
            --disable-debug
            --enable-optimizations
        )
    fi

    # Run configure
    ./configure "${CONFIGURE_OPTIONS[@]}"

    log_info "FFmpeg configured successfully"
}

# Build FFmpeg
build_ffmpeg() {
    log_info "Building FFmpeg..."
    cd "${SOURCE_DIR}"

    # Use all available cores
    local JOBS=$(sysctl -n hw.ncpu)
    make -j"${JOBS}"

    log_info "FFmpeg built successfully"
}

# Install FFmpeg
install_ffmpeg() {
    log_info "Installing FFmpeg to ${OUTPUT_DIR}..."
    cd "${SOURCE_DIR}"

    make install

    # Fix dylib install names for embedding
    log_info "Fixing dylib install names..."
    for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
        if [ -f "$dylib" ] && [ ! -L "$dylib" ]; then
            local name=$(basename "$dylib")
            install_name_tool -id "@rpath/${name}" "$dylib"
            log_info "Fixed: ${name}"
        fi
    done

    # Fix inter-library dependencies
    for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
        if [ -f "$dylib" ] && [ ! -L "$dylib" ]; then
            # Fix dependencies to other FFmpeg libraries
            for dep in libavcodec libavformat libavutil libswscale libswresample; do
                local dep_path=$(otool -L "$dylib" | grep "${dep}" | awk '{print $1}' | head -1)
                if [ -n "$dep_path" ] && [[ "$dep_path" != @rpath* ]]; then
                    local dep_name=$(basename "$dep_path")
                    install_name_tool -change "$dep_path" "@rpath/${dep_name}" "$dylib"
                fi
            done
        fi
    done

    log_info "FFmpeg installed successfully"
}

# Verify LGPL compliance
verify_lgpl() {
    log_info "Verifying LGPL compliance..."

    local config_file="${SOURCE_DIR}/ffbuild/config.mak"

    if [ ! -f "$config_file" ]; then
        log_error "Config file not found!"
        return 1
    fi

    # Check that GPL is NOT enabled
    if grep -q "CONFIG_GPL=yes" "$config_file"; then
        log_error "GPL is enabled! This build is NOT App Store compliant!"
        return 1
    fi

    # Check that nonfree is NOT enabled
    if grep -q "CONFIG_NONFREE=yes" "$config_file"; then
        log_error "Nonfree is enabled! This build is NOT App Store compliant!"
        return 1
    fi

    # Check for banned libraries
    local banned_libs=("libx264" "libx265" "libxvid" "libfdk" "libfaac")
    for lib in "${banned_libs[@]}"; do
        if grep -qi "${lib}=yes" "$config_file"; then
            log_error "Banned library found: ${lib}"
            return 1
        fi
    done

    log_info "✓ LGPL compliance verified"
    log_info "  - GPL: disabled"
    log_info "  - Nonfree: disabled"
    log_info "  - Banned libraries: none found"

    return 0
}

# Print build summary
print_summary() {
    log_info "========================================"
    log_info "FFmpeg Build Complete"
    log_info "========================================"
    log_info "Version: ${FFMPEG_VERSION}"
    log_info "Architecture: ${TARGET_ARCH}"
    log_info "Output: ${OUTPUT_DIR}"
    log_info ""
    log_info "Libraries built:"
    ls -la "${OUTPUT_DIR}/lib/"*.dylib 2>/dev/null || echo "  (no dylibs found)"
    log_info ""
    log_info "Next steps:"
    log_info "  1. Run verify-lgpl.sh to double-check compliance"
    log_info "  2. Run create-xcframework.sh to create XCFramework"
    log_info "========================================"
}

# Main build process
main() {
    log_info "Starting CYBFFmpeg FFmpeg build..."
    log_info "========================================"

    check_dependencies
    download_ffmpeg
    configure_ffmpeg
    build_ffmpeg
    install_ffmpeg
    verify_lgpl
    print_summary
}

main "$@"
