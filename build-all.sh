#!/bin/bash
# =============================================================================
# CYBFFmpeg — top-level build orchestrator
# =============================================================================
# Builds the two artifacts CYBFFmpeg ships out of source:
#   1. cyb-ffmpeg-core/target/release/libcyb_ffmpeg_core.a (Rust static lib)
#   2. ffmpeg-build/output/lib/lib*.dylib                  (FFmpeg dynamic libs)
#
# Both are gitignored. Consumers (e.g. CYBFFmpegDecoder, kirinuki-ai) link
# against them via paths set in CYBFFmpeg's Package.swift.
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
    if [ "$DEBUG_BUILD" = true ]; then
        cargo build
        echo "Built debug: target/debug/libcyb_ffmpeg_core.a"
    else
        cargo build --release
        echo "Built release: target/release/libcyb_ffmpeg_core.a"
    fi
fi

# ---- FFmpeg LGPL build -------------------------------------------------------
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

echo ""
echo "============================================="
echo "build-all.sh complete"
echo "============================================="
ls -lh "$SCRIPT_DIR/cyb-ffmpeg-core/target/release/libcyb_ffmpeg_core.a" 2>/dev/null || true
ls -lh "$SCRIPT_DIR/ffmpeg-build/output/lib/"*.dylib 2>/dev/null | head -10 || true
