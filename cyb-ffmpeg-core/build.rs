//! Build script for cyb-ffmpeg-core
//!
//! This script:
//! 1. Locates FFmpeg libraries using pkg-config
//! 2. Generates C headers using cbindgen
//! 3. Sets up link paths for static/dynamic linking

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    // Get build configuration
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // Find FFmpeg libraries
    find_ffmpeg_libs();

    // Generate C header
    generate_header(&manifest_dir, &out_dir);

    // macOS-specific frameworks
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=VideoToolbox");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=CoreVideo");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
    }
}

/// Find FFmpeg libraries using pkg-config or fallback paths
fn find_ffmpeg_libs() {
    // Try pkg-config first
    let libs = ["libavcodec", "libavformat", "libavutil", "libswscale"];

    let mut found_all = true;
    for lib in &libs {
        match pkg_config::Config::new()
            .atleast_version("58.0.0") // FFmpeg 6.0+
            .probe(lib)
        {
            Ok(library) => {
                println!("cargo:info=Found {} via pkg-config", lib);
                for path in &library.link_paths {
                    println!("cargo:rustc-link-search=native={}", path.display());
                }
            }
            Err(e) => {
                println!("cargo:warning=pkg-config failed for {}: {}", lib, e);
                found_all = false;
            }
        }
    }

    // Fallback to common installation paths on macOS
    if !found_all {
        try_fallback_paths();
    }
}

/// Try common FFmpeg installation paths
fn try_fallback_paths() {
    let homebrew_paths = [
        // Apple Silicon Homebrew
        "/opt/homebrew/opt/ffmpeg/lib",
        "/opt/homebrew/lib",
        // Intel Homebrew
        "/usr/local/opt/ffmpeg/lib",
        "/usr/local/lib",
        // Custom FFmpeg build (our xcframework location)
        "ffmpeg-build/output/lib",
    ];

    let homebrew_include = [
        "/opt/homebrew/opt/ffmpeg/include",
        "/opt/homebrew/include",
        "/usr/local/opt/ffmpeg/include",
        "/usr/local/include",
    ];

    // Check for existing paths
    for path in &homebrew_paths {
        if Path::new(path).exists() {
            println!("cargo:rustc-link-search=native={}", path);
            println!("cargo:info=Added link path: {}", path);
        }
    }

    for path in &homebrew_include {
        if Path::new(path).exists() {
            println!("cargo:include={}", path);
        }
    }

    // Link FFmpeg libraries dynamically
    println!("cargo:rustc-link-lib=dylib=avcodec");
    println!("cargo:rustc-link-lib=dylib=avformat");
    println!("cargo:rustc-link-lib=dylib=avutil");
    println!("cargo:rustc-link-lib=dylib=swscale");
}

/// Generate C header using cbindgen
fn generate_header(manifest_dir: &str, out_dir: &str) {
    let crate_dir = PathBuf::from(manifest_dir);
    let config_path = crate_dir.join("cbindgen.toml");

    // Output paths
    let header_out = PathBuf::from(out_dir).join("cyb_ffmpeg.h");
    let swift_header_dir = crate_dir
        .parent()
        .map(|p| p.join("Sources/CYBFFmpeg/CybFFmpegC/include"))
        .unwrap_or_else(|| crate_dir.join("include"));

    // Load cbindgen config
    let config = if config_path.exists() {
        cbindgen::Config::from_file(&config_path).unwrap_or_default()
    } else {
        cbindgen::Config::default()
    };

    // Generate header
    match cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
    {
        Ok(bindings) => {
            // Write to OUT_DIR
            bindings.write_to_file(&header_out);
            println!("cargo:info=Generated header: {}", header_out.display());

            // Also copy to Swift package location if it exists
            if swift_header_dir.exists() {
                let swift_header = swift_header_dir.join("cyb_ffmpeg_generated.h");
                bindings.write_to_file(&swift_header);
                println!(
                    "cargo:info=Copied header to Swift package: {}",
                    swift_header.display()
                );
            }
        }
        Err(e) => {
            println!("cargo:warning=cbindgen failed: {}", e);
            // Create a minimal header on failure
            create_fallback_header(out_dir);
        }
    }
}

/// Create a minimal fallback header if cbindgen fails
fn create_fallback_header(out_dir: &str) {
    let header_content = r#"
#ifndef CYB_FFMPEG_H
#define CYB_FFMPEG_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

// Note: This is a fallback header. Build with cbindgen for full API.

typedef enum {
    CybResultSuccess = 0,
    CybResultErrorFileNotFound = 1,
    CybResultErrorInvalidFormat = 2,
    CybResultErrorCodecNotSupported = 3,
    CybResultErrorDecodeFailed = 4,
    CybResultErrorSeekFailed = 5,
    CybResultErrorMemory = 6,
    CybResultErrorInvalidHandle = 7,
    CybResultErrorNotPrepared = 8,
    CybResultErrorUnknown = 99,
} CybResult;

typedef struct CybDecoderHandle CybDecoderHandle;
typedef struct CybFrameHandle CybFrameHandle;
typedef struct CybMediaInfoHandle CybMediaInfoHandle;

// Error handling
const char* cyb_get_last_error(void);
void cyb_clear_last_error(void);

// Version info
const char* cyb_get_version(void);
const char* cyb_get_ffmpeg_version(void);

// Decoder lifecycle
CybDecoderHandle* cyb_decoder_create(const char* path, const void* config);
CybResult cyb_decoder_prepare(CybDecoderHandle* handle);
void cyb_decoder_destroy(CybDecoderHandle* handle);
bool cyb_decoder_is_prepared(const CybDecoderHandle* handle);

// Decoding
CybResult cyb_decoder_start(CybDecoderHandle* handle);
CybResult cyb_decoder_stop(CybDecoderHandle* handle);
CybResult cyb_decoder_seek(CybDecoderHandle* handle, int64_t time_us);

// Frame operations
CybResult cyb_decoder_get_frame_at(CybDecoderHandle* handle, int64_t time_us,
                                   int64_t tolerance_us, CybFrameHandle** out_frame);
CybResult cyb_decoder_get_next_frame(CybDecoderHandle* handle, CybFrameHandle** out_frame);
void cyb_frame_release(CybFrameHandle* frame);

// Media info
CybResult cyb_decoder_get_media_info(const CybDecoderHandle* handle,
                                     CybMediaInfoHandle** out_info);
void cyb_media_info_release(CybMediaInfoHandle* info);

#endif // CYB_FFMPEG_H
"#;

    let header_path = PathBuf::from(out_dir).join("cyb_ffmpeg.h");
    std::fs::write(&header_path, header_content).expect("Failed to write fallback header");
    println!("cargo:info=Created fallback header: {}", header_path.display());
}
