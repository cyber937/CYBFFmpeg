#!/bin/bash
# =============================================================================
# CYBFFmpeg — XCFramework packager
# =============================================================================
# Freezes the *native* layer (FFmpeg 7.1 dynamic libs + Rust static core) into
# pre-built XCFrameworks so consumers (CYBFFmpegDecoder / CYBMediaPlayer /
# kirinuki-ai) link ready-made binaries instead of recompiling FFmpeg (30-60
# min) and the Rust crate, and no longer need the sibling-clone source layout.
#
# Produces under ./dist :
#   CybFFmpegCore.xcframework   ← Rust libcyb_ffmpeg_core.a + headers/modulemap
#                                 (module `CybFFmpegC`, our code, STATIC)
#   avcodec.xcframework    ┐
#   avformat.xcframework   │     FFmpeg libs as DYNAMIC xcframeworks — kept
#   avutil.xcframework     ├──   dynamic on purpose for LGPL v3.0 compliance
#   swscale.xcframework    │     (user must be able to replace them). Shipped
#   swresample.xcframework ┘     unmodified except @rpath install-name fixup.
#
# Plus <name>.xcframework.zip + a printed `.binaryTarget(url:checksum:)` block
# to paste into Package.swift after uploading the zips to a GitHub Release.
#
# Prerequisite: ./build-all.sh has produced the dylibs + Rust .a. This script
# does NOT recompile anything — it only repackages existing artifacts.
#
# Usage: ./make-xcframeworks.sh
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

FFMPEG_LIB_DIR="ffmpeg-build/output/lib"
RUST_LIB="cyb-ffmpeg-core/target/release/libcyb_ffmpeg_core.a"
HEADER_DIR="Sources/CYBFFmpeg/CybFFmpegC/include"
DIST="dist"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FFMPEG_LIBS=(avcodec avformat avutil swscale swresample)

# ---- preflight --------------------------------------------------------------
[ -f "$RUST_LIB" ] || { echo "Error: $RUST_LIB missing — run ./build-all.sh first." >&2; exit 1; }
for l in "${FFMPEG_LIBS[@]}"; do
    [ -e "$FFMPEG_LIB_DIR/lib$l.dylib" ] || { echo "Error: lib$l.dylib missing in $FFMPEG_LIB_DIR — run ./build-all.sh first." >&2; exit 1; }
done

rm -rf "$DIST"
mkdir -p "$DIST"

echo "=== Staging + normalizing FFmpeg dylibs (@rpath, symlink-free names) ==="
for l in "${FFMPEG_LIBS[@]}"; do
    out="$WORK/lib$l.dylib"
    # -L follows the version symlink and copies the real binary content.
    cp -L "$FFMPEG_LIB_DIR/lib$l.dylib" "$out"
    chmod u+w "$out"
    # Canonical, version-free install name so no version symlinks are needed
    # once the dylib is embedded in Contents/Frameworks/.
    install_name_tool -id "@rpath/lib$l.dylib" "$out"
    # Rewrite inter-FFmpeg dependencies (e.g. @rpath/libavutil.59.dylib) to the
    # same canonical names.
    while IFS= read -r dep; do
        for base in "${FFMPEG_LIBS[@]}"; do
            case "$dep" in
                @rpath/lib$base.*.dylib)
                    install_name_tool -change "$dep" "@rpath/lib$base.dylib" "$out"
                    ;;
            esac
        done
    done < <(otool -L "$out" | awk 'NR>1{print $1}')
    # install_name_tool invalidates the signature; ad-hoc re-sign (Xcode
    # re-signs with the app identity on embed, but keep it valid standalone).
    codesign -f -s - "$out" >/dev/null 2>&1 || true
    xcodebuild -create-xcframework -library "$out" -output "$DIST/$l.xcframework" >/dev/null
    echo "  built $DIST/$l.xcframework"
done

echo "=== Packaging Rust static core (module CybFFmpegC) ==="
CORE_HDR="$WORK/coreheaders"
mkdir -p "$CORE_HDR"
cp "$HEADER_DIR/cyb_ffmpeg.h" "$CORE_HDR/"
[ -f "$HEADER_DIR/cyb_ffmpeg_generated.h" ] && cp "$HEADER_DIR/cyb_ffmpeg_generated.h" "$CORE_HDR/"
# modulemap WITHOUT `link "cyb_ffmpeg_core"` — the static lib is supplied by the
# XCFramework itself, so an explicit -l would double-link / fail to resolve.
cat > "$CORE_HDR/module.modulemap" <<'EOF'
module CybFFmpegC {
    header "cyb_ffmpeg.h"
    export *
}
EOF
cp "$RUST_LIB" "$WORK/libcyb_ffmpeg_core.a"
xcodebuild -create-xcframework \
    -library "$WORK/libcyb_ffmpeg_core.a" \
    -headers "$CORE_HDR" \
    -output "$DIST/CybFFmpegCore.xcframework" >/dev/null
echo "  built $DIST/CybFFmpegCore.xcframework"

echo "=== Zipping + computing checksums ==="
printf '\n----- paste into Package.swift (after uploading zips to the Release) -----\n'
for x in CybFFmpegCore "${FFMPEG_LIBS[@]}"; do
    ditto -c -k --keepParent "$DIST/$x.xcframework" "$DIST/$x.xcframework.zip"
    sum="$(swift package compute-checksum "$DIST/$x.xcframework.zip")"
    printf '    nativeBinary("%s", "%s"),\n' "$x" "$sum"
done
printf -- '-------------------------------------------------------------------------\n\n'
echo "Done. Artifacts in ./$DIST"
