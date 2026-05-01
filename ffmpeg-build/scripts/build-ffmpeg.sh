#!/bin/bash
# =============================================================================
# CYBFFmpeg - LGPL v3.0 Compliant FFmpeg Build Script
# =============================================================================
# This script builds FFmpeg with LGPL-only components for Mac App Store
# distribution. NO GPL components (libx264, libx265, etc.) are included.
# All produced dylibs are self-contained (no Homebrew or external paths).
#
# Supported codecs (LGPL native + system frameworks only):
# - MPEG-1/2/4 - Native LGPL
# - DNxHD/HR - Native LGPL
# - H.264/HEVC - VideoToolbox (Apple system framework)
# - VP9 - VideoToolbox HW decode on Apple Silicon (no software fallback)
# - AV1 - VideoToolbox HW decode on M3+ (no software fallback)
# - ProRes - Native LGPL
# - WMV1/2/3/VC-1 - Native LGPL (Windows Media Video)
# - WMA - Native LGPL (Windows Media Audio)
# - AC-3/E-AC-3 - Native LGPL (Dolby Digital)
#
# External libraries (libvpx, libdav1d, etc.) are intentionally NOT linked.
# This keeps the dylibs free of /opt/homebrew/... absolute path dependencies
# so they can be embedded into a sandboxed Mac App Store app and re-signed
# with the consumer app's Team ID for library validation compliance.
#
# Usage:
#   ./build-ffmpeg.sh [--clean] [--debug]
#
# =============================================================================

set -e

# Configuration.
#
# DECISION (2026-05-01): Pinned to the 7.1 series. See:
#   - docs/planning/mxf-codec-support.md (kirinuki-ai)
#   - cyb-ffmpeg-core/Cargo.toml (paired pin on ffmpeg-next 7.1)
# 8.0.1 + ffmpeg-next 8.0.0 hit non-exhaustive enum match errors
# (`AV_PKT_DATA_EXIF`). 7.1 covers all features NexClip needs.
FFMPEG_VERSION="7.1.2"
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

# Check for build tools (only nasm + pkg-config). External codec libraries
# are intentionally not linked — see header for rationale.
check_dependencies() {
    log_info "Checking dependencies..."

    local missing_deps=()

    # Required build tools
    for tool in pkg-config nasm; do
        if ! command -v "$tool" &> /dev/null; then
            missing_deps+=("$tool")
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

        # External codec libraries are intentionally NOT linked.
        # libvpx (VP9) and libdav1d (AV1) would pull in /opt/homebrew/...
        # absolute path dependencies that App Store sandbox + library
        # validation reject. VP9 / AV1 fall back to VideoToolbox HW decode
        # on Apple Silicon. See header for codec support matrix.

        # Disable X11 / xcb auto-detection (FFmpeg's configure may detect
        # them from Homebrew via libdav1d's transitive deps even when we
        # don't explicitly enable them).
        --disable-libxcb
        --disable-libxcb-shm
        --disable-libxcb-xfixes
        --disable-libxcb-shape

        # Disable SDL2 — Homebrew's sdl2 has a transitive libX11 dependency
        # which leaks /opt/homebrew/opt/libx11/lib/libX11.6.dylib into every
        # FFmpeg dylib. NexClip uses VideoToolbox / AppKit / CoreImage for
        # rendering, never SDL2.
        --disable-sdl2

        # Defense in depth: explicitly disable anything else that could
        # transitively pull X11 from Homebrew. None of these are used by
        # NexClip, and FFmpeg's configure happily auto-enables them when
        # pkg-config sees the Homebrew formulas.
        --disable-libdrm
        --disable-vaapi
        --disable-vdpau

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
        --enable-demuxer=asf       # Windows Media / ASF container
        --enable-demuxer=avi       # AVI container (ensure enabled)
        --enable-demuxer=wav       # WAV audio container
        --enable-demuxer=flac      # FLAC container
        --enable-demuxer=ogg       # Ogg container

        # Enable parsers
        --enable-parser=h264
        --enable-parser=hevc
        --enable-parser=vp9
        --enable-parser=av1
        --enable-parser=mpeg4video
        --enable-parser=mpegvideo
        --enable-parser=vc1         # VC-1 / WMV9 parser

        # Enable LGPL decoders
        --enable-decoder=h264
        --enable-decoder=hevc
        --enable-decoder=vp9          # FFmpeg native software decoder (slow on M1)
        --enable-decoder=av1          # FFmpeg native software decoder (slow without dav1d)
        --enable-decoder=mpeg1video
        --enable-decoder=mpeg2video
        --enable-decoder=mpeg4
        --enable-decoder=prores
        --enable-decoder=dnxhd
        --enable-decoder=rawvideo
        --enable-decoder=wmv1       # Windows Media Video 7
        --enable-decoder=wmv2       # Windows Media Video 8
        --enable-decoder=wmv3       # Windows Media Video 9
        --enable-decoder=vc1        # SMPTE VC-1
        --enable-decoder=wmv3image  # WMV9 Image
        --enable-decoder=vc1image   # VC-1 Image
        --enable-decoder=msmpeg4v1  # MS MPEG-4 v1
        --enable-decoder=msmpeg4v2  # MS MPEG-4 v2
        --enable-decoder=msmpeg4v3  # MS MPEG-4 v3 (DivX 3)

        # Audio decoders (all LGPL)
        --enable-decoder=aac
        --enable-decoder=mp3
        --enable-decoder=flac
        --enable-decoder=pcm_s16le
        --enable-decoder=pcm_s24le
        --enable-decoder=pcm_s32le
        --enable-decoder=pcm_f32le
        --enable-decoder=wmav1      # Windows Media Audio v1
        --enable-decoder=wmav2      # Windows Media Audio v2
        --enable-decoder=wmalossless # WMA Lossless
        --enable-decoder=wmapro     # WMA Pro
        --enable-decoder=wmavoice   # WMA Voice
        --enable-decoder=vorbis     # Vorbis (for Ogg)
        --enable-decoder=opus       # Opus audio
        --enable-decoder=ac3        # AC-3 / Dolby Digital
        --enable-decoder=eac3       # E-AC-3
        --enable-decoder=mp2        # MPEG Layer 2 audio
        --enable-decoder=mp2float   # MP2 float decoder

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

    # Run configure with an explicitly empty PKG_CONFIG_PATH so FFmpeg
    # cannot pick up Homebrew (or any other) external libraries at
    # auto-detection time. Combined with the explicit --disable-libxcb*
    # and --disable-libvpx/--disable-libdav1d defaults, this guarantees
    # the produced dylibs only depend on system frameworks.
    PKG_CONFIG_PATH= ./configure "${CONFIGURE_OPTIONS[@]}"

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

# Verify produced dylibs are self-contained — they must not depend on any
# path outside /usr/lib (system) or @rpath (bundled). Catches Homebrew /
# /opt/local / /usr/local leakage that would break App Store distribution.
verify_self_contained() {
    log_info "Verifying dylibs have no external path dependencies..."

    local violations=0
    for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
        if [ -f "$dylib" ] && [ ! -L "$dylib" ]; then
            local bad_deps
            bad_deps=$(otool -L "$dylib" | tail -n +2 | awk '{print $1}' | \
                grep -E '^(/opt/|/usr/local/|/Users/|/Library/|/Volumes/)' || true)
            if [ -n "$bad_deps" ]; then
                log_error "$(basename "$dylib") has external path dependencies:"
                echo "$bad_deps" | sed 's/^/    /'
                violations=$((violations + 1))
            fi
        fi
    done

    if [ "$violations" -gt 0 ]; then
        log_error "Found $violations dylib(s) with external path dependencies."
        log_error "App Store distribution requires self-contained dylibs."
        return 1
    fi

    log_info "✓ All dylibs are self-contained"
    return 0
}

# Verify LGPL compliance
verify_lgpl() {
    log_info "Verifying LGPL compliance..."

    local config_file="${SOURCE_DIR}/ffbuild/config.mak"

    if [ ! -f "$config_file" ]; then
        log_error "Config file not found!"
        return 1
    fi

    # Check that GPL is NOT enabled.
    # FFmpeg 7.x writes `CONFIG_GPL=yes` only when GPL is enabled (absent
    # otherwise). FFmpeg 8.x additionally writes `!CONFIG_GPL=yes` when
    # disabled. We check for the line WITHOUT the leading "!" so the test
    # works on both 7.x and 8.x.
    if grep -q "^CONFIG_GPL=yes" "$config_file"; then
        log_error "GPL is enabled! This build is NOT App Store compliant!"
        return 1
    fi

    # Check that nonfree is NOT enabled
    if grep -q "^CONFIG_NONFREE=yes" "$config_file"; then
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
    verify_self_contained
    verify_lgpl
    print_summary
}

main "$@"
