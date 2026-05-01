#!/bin/bash
# =============================================================================
# CYBFFmpeg — top-level build orchestrator
# =============================================================================
# Builds the two artifacts CYBFFmpeg ships out of source:
#   1. ffmpeg-build/output/lib/lib*.dylib                  (FFmpeg dynamic libs)
#   2. cyb-ffmpeg-core/target/release/libcyb_ffmpeg_core.a (Rust static lib)
#
# Both are gitignored. Consumers (e.g. CYBFFmpegDecoder, kirinuki-ai) link
# against them via paths set in CYBFFmpeg's Package.swift.
#
# Build order matters: FFmpeg must be built *first* because the Rust crate
# `ffmpeg-sys-next` runs bindgen against the FFmpeg headers. We point its
# pkg-config at our locally built FFmpeg so the Rust core links against the
# same LGPL-clean libraries we ship in the dylibs — not whatever Homebrew
# happens to have installed (which could be GPL-tainted, the wrong major
# version, or absent).
#
# Usage:
#   ./build-all.sh [--clean] [--debug] [--skip-rust] [--skip-ffmpeg]
#
# First run: 30–60 minutes (clean FFmpeg compile dominates).
# Subsequent runs: a few seconds (FFmpeg's source tree is cached).
#
# Distribution constraints:
#   - LGPL v3.0 only, no GPL components
#   - Self-contained dylibs (no /opt/homebrew/... absolute path deps)
#   - arm64 only (Apple Silicon)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CLEAN_BUILD=false
DEBUG_BUILD=false
SKIP_RUST=false
SKIP_FFMPEG=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --clean)       CLEAN_BUILD=true;  shift ;;
        --debug)       DEBUG_BUILD=true;  shift ;;
        --skip-rust)   SKIP_RUST=true;    shift ;;
        --skip-ffmpeg) SKIP_FFMPEG=true;  shift ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: $0 [--clean] [--debug] [--skip-rust] [--skip-ffmpeg]" >&2
            exit 1
            ;;
    esac
done

# ---- FFmpeg LGPL build (must run first — Rust depends on its headers) -------
if [ "$SKIP_FFMPEG" = false ]; then
    echo ""
    echo "============================================="
    echo "Building FFmpeg (LGPL v3.0, self-contained)"
    echo "============================================="
    cd "$SCRIPT_DIR/ffmpeg-build/scripts"
    FFMPEG_ARGS=()
    [ "$CLEAN_BUILD" = true ] && FFMPEG_ARGS+=(--clean)
    [ "$DEBUG_BUILD" = true ] && FFMPEG_ARGS+=(--debug)
    ./build-ffmpeg.sh "${FFMPEG_ARGS[@]}"
fi

# ---- Rust core ---------------------------------------------------------------
if [ "$SKIP_RUST" = false ]; then
    echo ""
    echo "============================================="
    echo "Building cyb-ffmpeg-core (Rust static lib)"
    echo "============================================="
    cd "$SCRIPT_DIR/cyb-ffmpeg-core"
    if [ "$CLEAN_BUILD" = true ]; then
        cargo clean
    fi

    # Point ffmpeg-sys-next at our locally built FFmpeg, *exclusively*. Empty
    # PKG_CONFIG_PATH_DEFAULT plus a single explicit PKG_CONFIG_PATH entry
    # prevents pkg-config from falling back to Homebrew (which would otherwise
    # supply a different major version and break bindgen + ffmpeg-next's
    # version-gated `match` arms).
    LOCAL_PC="$SCRIPT_DIR/ffmpeg-build/output/lib/pkgconfig"
    if [ ! -d "$LOCAL_PC" ]; then
        echo "Error: $LOCAL_PC not found — did you run build-ffmpeg.sh first?" >&2
        echo "Re-run without --skip-ffmpeg or build-ffmpeg.sh manually." >&2
        exit 1
    fi
    export PKG_CONFIG_PATH="$LOCAL_PC"
    export PKG_CONFIG_PATH_FOR_TARGET="$LOCAL_PC"
    export PKG_CONFIG_LIBDIR="$LOCAL_PC"

    # Match CYBFFmpeg's Package.swift `.macOS(.v14)` and the FFmpeg build
    # above. Without this, cargo would fall back to the host SDK's min
    # (e.g. 26.0 on Tahoe-beta) and the static lib would carry an
    # LC_BUILD_VERSION incompatible with consumer apps targeting 14.x.
    export MACOSX_DEPLOYMENT_TARGET="14.0"

    if [ "$DEBUG_BUILD" = true ]; then
        cargo build
        echo "Built debug: target/debug/libcyb_ffmpeg_core.a"
    else
        cargo build --release
        echo "Built release: target/release/libcyb_ffmpeg_core.a"
    fi
fi

echo ""
echo "============================================="
echo "build-all.sh complete"
echo "============================================="
ls -lh "$SCRIPT_DIR/cyb-ffmpeg-core/target/release/libcyb_ffmpeg_core.a" 2>/dev/null || true
ls -lh "$SCRIPT_DIR/ffmpeg-build/output/lib/"*.dylib 2>/dev/null | head -10 || true
