//! CYBFFmpeg Core - Rust implementation for FFmpeg-based video decoding
//!
//! This crate provides the core functionality for CYBFFmpeg:
//! - FFmpeg wrapper using ffmpeg-next
//! - Multi-tier frame caching (L1/L2/L3)
//! - Parallel decoding and prefetching
//! - VideoToolbox hardware acceleration
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │           FFI Layer                  │
//! │  (C exports via #[no_mangle])        │
//! └─────────────────────────────────────┘
//!                  │
//!                  ▼
//! ┌─────────────────────────────────────┐
//! │         Decoder Module               │
//! │  (ffmpeg-next wrapper)               │
//! └─────────────────────────────────────┘
//!                  │
//!                  ▼
//! ┌─────────────────────────────────────┐
//! │          Cache Module                │
//! │  (L1/L2/L3 multi-tier cache)         │
//! └─────────────────────────────────────┘
//!                  │
//!                  ▼
//! ┌─────────────────────────────────────┐
//! │        Threading Module              │
//! │  (prefetch workers)                  │
//! └─────────────────────────────────────┘
//! ```

pub mod cache;
pub mod decoder;
pub mod error;
pub mod ffi;
pub mod threading;

// Re-export main types
pub use cache::{Cache, CacheConfig, CacheStatistics};
pub use decoder::{Decoder, DecoderConfig, MediaInfo, VideoFrame};
pub use error::{Error, Result};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the library (call once at startup)
pub fn init() {
    // Initialize logging with info level by default if RUST_LOG is not set
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).try_init();

    // FFmpeg initialization happens automatically with ffmpeg-next
    log::info!("CYBFFmpeg Core {} initialized", VERSION);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_init() {
        init();
        // Should not panic
    }
}
