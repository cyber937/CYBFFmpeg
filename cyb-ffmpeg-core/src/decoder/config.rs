//! Decoder configuration

/// Pixel format for output frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PixelFormat {
    /// BGRA (32-bit, Metal optimized)
    Bgra = 0,
    /// NV12 (12-bit, VideoToolbox native)
    Nv12 = 1,
    /// YUV420P (12-bit, planar)
    Yuv420p = 2,
}

impl Default for PixelFormat {
    fn default() -> Self {
        // NV12 over BGRA: 12-bit vs 32-bit per pixel = 62% less memory
        // bandwidth across the Rust→Swift FFI for 4K frames. yuv420p →
        // NV12 in SWS is also lighter than yuv420p → BGRA.
        Self::Nv12
    }
}

/// Decoder configuration
#[derive(Debug, Clone)]
pub struct DecoderConfig {
    /// Prefer hardware decoding via VideoToolbox
    pub prefer_hardware_decoding: bool,

    /// L1 cache capacity (hot frames)
    pub l1_cache_capacity: u32,

    /// L2 cache capacity (keyframes)
    pub l2_cache_capacity: u32,

    /// L3 cache capacity (cold frames)
    pub l3_cache_capacity: u32,

    /// Enable background prefetching
    pub enable_prefetch: bool,

    /// Number of decoding threads (0 = auto)
    pub thread_count: u32,

    /// Output pixel format
    pub output_pixel_format: PixelFormat,

    /// When true, `Decoder::prepare()` skips the up-front keyframe-index
    /// scan. The index enables byte-precise seeks (used internally by
    /// `seek_precise`); building it requires walking the entire video
    /// stream once, which can take 20+ seconds on cold 4K MXF I/O.
    ///
    /// Use this when the decoder is created purely for metadata/timecode
    /// extraction and will be discarded without any seek/playback. The
    /// timecode tag lives on format/stream metadata and is available
    /// immediately after `find_stream_info`, so a probe-only flow is
    /// ~20× faster with this set.
    ///
    /// Default `false` to preserve playback behavior for existing callers.
    pub skip_keyframe_indexing: bool,

    /// Optional path to a disk cache file for the keyframe index.
    ///
    /// When set, `Decoder::prepare()` will:
    /// 1. Try to load a previously-saved index from this path. The cache
    ///    file embeds the source file's `(size, mtime_secs)` and is
    ///    rejected if either differs from the current source file.
    /// 2. On cache miss, build the index normally and write it to this
    ///    path before returning, so subsequent calls hit the cache.
    ///
    /// Skipped entirely when `skip_keyframe_indexing` is `true`.
    ///
    /// Default `None` preserves current behavior (always rebuild).
    pub keyframe_index_cache_path: Option<String>,
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self {
            prefer_hardware_decoding: true,
            l1_cache_capacity: 30,
            l2_cache_capacity: 100,
            l3_cache_capacity: 500,
            enable_prefetch: true,
            thread_count: 0,
            output_pixel_format: PixelFormat::Nv12,
            skip_keyframe_indexing: false,
            keyframe_index_cache_path: None,
        }
    }
}

impl DecoderConfig {
    /// Performance preset with larger caches
    pub fn performance() -> Self {
        Self {
            prefer_hardware_decoding: true,
            l1_cache_capacity: 60,
            l2_cache_capacity: 200,
            l3_cache_capacity: 1000,
            enable_prefetch: true,
            thread_count: 0,
            output_pixel_format: PixelFormat::Nv12,
            skip_keyframe_indexing: false,
            keyframe_index_cache_path: None,
        }
    }

    /// Low memory preset
    pub fn low_memory() -> Self {
        Self {
            prefer_hardware_decoding: true,
            l1_cache_capacity: 15,
            l2_cache_capacity: 50,
            l3_cache_capacity: 100,
            enable_prefetch: false,
            thread_count: 2,
            output_pixel_format: PixelFormat::Nv12,
            skip_keyframe_indexing: false,
            keyframe_index_cache_path: None,
        }
    }

    /// Scrubbing optimized preset
    pub fn scrubbing() -> Self {
        Self {
            prefer_hardware_decoding: true,
            l1_cache_capacity: 45,
            l2_cache_capacity: 200,
            l3_cache_capacity: 800,
            enable_prefetch: true,
            thread_count: 0,
            output_pixel_format: PixelFormat::Nv12,
            skip_keyframe_indexing: false,
            keyframe_index_cache_path: None,
        }
    }

    /// Metadata-only preset — skips the keyframe index for fast probes that
    /// will not perform any seek or playback. Cache sizes are set to zero
    /// because the decoder is expected to be discarded immediately after
    /// reading metadata.
    pub fn metadata_only() -> Self {
        Self {
            prefer_hardware_decoding: false,
            l1_cache_capacity: 0,
            l2_cache_capacity: 0,
            l3_cache_capacity: 0,
            enable_prefetch: false,
            thread_count: 1,
            output_pixel_format: PixelFormat::Nv12,
            skip_keyframe_indexing: true,
            keyframe_index_cache_path: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DecoderConfig::default();
        assert!(config.prefer_hardware_decoding);
        assert_eq!(config.l1_cache_capacity, 30);
        assert!(config.enable_prefetch);
    }

    #[test]
    fn test_presets() {
        let perf = DecoderConfig::performance();
        assert_eq!(perf.l1_cache_capacity, 60);

        let low = DecoderConfig::low_memory();
        assert!(!low.enable_prefetch);
    }

    #[test]
    fn test_metadata_only_preset_skips_keyframe_indexing() {
        let metadata = DecoderConfig::metadata_only();
        assert!(metadata.skip_keyframe_indexing);
        assert_eq!(metadata.l1_cache_capacity, 0);
        assert!(!metadata.enable_prefetch);
    }

    #[test]
    fn test_default_does_not_skip_indexing() {
        let cfg = DecoderConfig::default();
        assert!(!cfg.skip_keyframe_indexing);
    }

    #[test]
    fn test_default_has_no_keyframe_index_cache_path() {
        let cfg = DecoderConfig::default();
        assert!(cfg.keyframe_index_cache_path.is_none());
    }

    #[test]
    fn test_metadata_only_has_no_cache_path() {
        let cfg = DecoderConfig::metadata_only();
        assert!(cfg.keyframe_index_cache_path.is_none());
    }
}
