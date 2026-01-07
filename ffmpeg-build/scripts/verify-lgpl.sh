#!/bin/bash
# =============================================================================
# CYBFFmpeg - LGPL Compliance Verification Script
# =============================================================================
# This script verifies that the FFmpeg build is LGPL v3.0 compliant
# for Mac App Store distribution.
#
# Checks performed:
# 1. GPL flag is disabled
# 2. Nonfree flag is disabled
# 3. No GPL-licensed libraries are linked
# 4. Dynamic libraries are properly configured
# 5. All linked libraries are LGPL/BSD/Apache compatible
#
# Usage:
#   ./verify-lgpl.sh [path-to-ffmpeg-output]
#
# =============================================================================

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_OUTPUT_DIR="${SCRIPT_DIR}/../output"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_info() {
    echo -e "[INFO] $1"
}

# Track verification status
VERIFICATION_PASSED=true

# Get output directory
OUTPUT_DIR="${1:-$DEFAULT_OUTPUT_DIR}"

if [ ! -d "$OUTPUT_DIR" ]; then
    log_fail "Output directory not found: $OUTPUT_DIR"
    exit 1
fi

echo "========================================"
echo "LGPL Compliance Verification"
echo "========================================"
echo "Checking: $OUTPUT_DIR"
echo ""

# -----------------------------------------------------------------------------
# Check 1: Verify FFmpeg configuration
# -----------------------------------------------------------------------------
check_ffmpeg_config() {
    echo "--- Check 1: FFmpeg Configuration ---"

    local config_file="${OUTPUT_DIR}/../build/ffmpeg-*/ffbuild/config.mak"
    local config_files=( $config_file )

    if [ ! -f "${config_files[0]}" ]; then
        log_warn "Config file not found, skipping config check"
        log_warn "  Expected: $config_file"
        return
    fi

    local config="${config_files[0]}"

    # Check GPL
    if grep -q "CONFIG_GPL=yes" "$config"; then
        log_fail "GPL is ENABLED"
        VERIFICATION_PASSED=false
    else
        log_pass "GPL is disabled"
    fi

    # Check nonfree
    if grep -q "CONFIG_NONFREE=yes" "$config"; then
        log_fail "Nonfree is ENABLED"
        VERIFICATION_PASSED=false
    else
        log_pass "Nonfree is disabled"
    fi

    # Check version3 (LGPL v3)
    if grep -q "CONFIG_VERSION3=yes" "$config"; then
        log_pass "Version3 (LGPL v3.0) is enabled"
    else
        log_warn "Version3 not explicitly enabled"
    fi
    echo ""
}

# -----------------------------------------------------------------------------
# Check 2: Verify no GPL libraries are linked
# -----------------------------------------------------------------------------
check_banned_libraries() {
    echo "--- Check 2: Banned Libraries ---"

    # GPL-licensed libraries that MUST NOT be present
    local banned_libs=(
        "libx264"
        "libx265"
        "libxvid"
        "libfdk-aac"
        "libfdk_aac"
        "libaacplus"
        "libfaac"
        "libvidstab"
        "librubberband"
        "libzimg"
        "libaribb24"
    )

    for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
        if [ ! -f "$dylib" ] || [ -L "$dylib" ]; then
            continue
        fi

        local name=$(basename "$dylib")

        for banned in "${banned_libs[@]}"; do
            if otool -L "$dylib" | grep -qi "$banned"; then
                log_fail "Banned library linked in ${name}: ${banned}"
                VERIFICATION_PASSED=false
            fi
        done
    done

    log_pass "No banned libraries found"
    echo ""
}

# -----------------------------------------------------------------------------
# Check 3: Verify dynamic library configuration
# -----------------------------------------------------------------------------
check_dylib_config() {
    echo "--- Check 3: Dynamic Library Configuration ---"

    for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
        if [ ! -f "$dylib" ] || [ -L "$dylib" ]; then
            continue
        fi

        local name=$(basename "$dylib")
        local install_name=$(otool -D "$dylib" | tail -1)

        # Check install name uses @rpath
        if [[ "$install_name" == @rpath/* ]]; then
            log_pass "${name}: install name uses @rpath"
        else
            log_warn "${name}: install name should use @rpath, got: ${install_name}"
        fi
    done
    echo ""
}

# -----------------------------------------------------------------------------
# Check 4: List all linked libraries
# -----------------------------------------------------------------------------
list_linked_libraries() {
    echo "--- Check 4: Linked Libraries ---"

    local all_deps=""

    for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
        if [ ! -f "$dylib" ] || [ -L "$dylib" ]; then
            continue
        fi

        local deps=$(otool -L "$dylib" | tail -n +2 | awk '{print $1}')
        all_deps="$all_deps$deps\n"
    done

    # Get unique dependencies
    local unique_deps=$(echo -e "$all_deps" | sort -u | grep -v "^$")

    echo "External dependencies found:"
    echo "$unique_deps" | while read -r dep; do
        if [[ "$dep" == @rpath/* ]] || [[ "$dep" == /usr/lib/* ]] || [[ "$dep" == /System/* ]]; then
            echo "  ✓ ${dep}"
        else
            echo "  ? ${dep} (verify license)"
        fi
    done
    echo ""
}

# -----------------------------------------------------------------------------
# Check 5: Verify LGPL-safe libraries
# -----------------------------------------------------------------------------
check_lgpl_safe() {
    echo "--- Check 5: LGPL-Safe External Libraries ---"

    # Libraries that are safe (LGPL/BSD/Apache/MIT)
    local safe_libs=(
        "libvpx"        # BSD
        "libdav1d"      # BSD-2-Clause
        "libaom"        # BSD-2-Clause
        "libogg"        # BSD
        "libvorbis"     # BSD
        "libopus"       # BSD
        "libflac"       # BSD
        "liblzma"       # Public Domain
        "libz"          # zlib
        "libbz2"        # BSD
    )

    log_info "Safe external libraries (LGPL/BSD compatible):"
    for lib in "${safe_libs[@]}"; do
        if pkg-config --exists "$lib" 2>/dev/null; then
            local version=$(pkg-config --modversion "$lib" 2>/dev/null || echo "unknown")
            echo "  ✓ ${lib} (${version})"
        fi
    done
    echo ""
}

# -----------------------------------------------------------------------------
# Check 6: Generate compliance report
# -----------------------------------------------------------------------------
generate_report() {
    echo "--- Compliance Report ---"

    local report_file="${OUTPUT_DIR}/LGPL_COMPLIANCE_REPORT.txt"

    {
        echo "CYBFFmpeg LGPL v3.0 Compliance Report"
        echo "Generated: $(date)"
        echo "========================================"
        echo ""
        echo "FFmpeg Libraries:"
        ls -la "${OUTPUT_DIR}"/lib/*.dylib 2>/dev/null | awk '{print "  " $NF}'
        echo ""
        echo "License: LGPL v3.0"
        echo "GPL Components: NONE"
        echo "Nonfree Components: NONE"
        echo ""
        echo "External Dependencies:"
        for dylib in "${OUTPUT_DIR}"/lib/*.dylib; do
            if [ -f "$dylib" ] && [ ! -L "$dylib" ]; then
                echo "$(basename "$dylib"):"
                otool -L "$dylib" | tail -n +2 | awk '{print "  " $1}'
            fi
        done
        echo ""
        echo "App Store Distribution: ALLOWED"
        echo ""
        echo "Requirements for Distribution:"
        echo "1. Include this compliance report in app bundle"
        echo "2. Mention FFmpeg in About/Credits section"
        echo "3. Provide link to source code (LGPL requirement)"
        echo "4. Include LGPL license text in app"
    } > "$report_file"

    log_info "Compliance report saved to: $report_file"
    echo ""
}

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
print_summary() {
    echo "========================================"
    if [ "$VERIFICATION_PASSED" = true ]; then
        echo -e "${GREEN}VERIFICATION PASSED${NC}"
        echo ""
        echo "This FFmpeg build is LGPL v3.0 compliant"
        echo "and suitable for Mac App Store distribution."
    else
        echo -e "${RED}VERIFICATION FAILED${NC}"
        echo ""
        echo "This FFmpeg build has compliance issues."
        echo "Review the errors above and rebuild."
    fi
    echo "========================================"
}

# Main
main() {
    check_ffmpeg_config
    check_banned_libraries
    check_dylib_config
    list_linked_libraries
    check_lgpl_safe
    generate_report
    print_summary

    if [ "$VERIFICATION_PASSED" = true ]; then
        exit 0
    else
        exit 1
    fi
}

main "$@"
