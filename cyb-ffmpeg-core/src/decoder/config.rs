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
        Self::Bgra
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
            output_pixel_format: PixelFormat::Bgra,
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
            output_pixel_format: PixelFormat::Bgra,
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
            output_pixel_format: PixelFormat::Bgra,
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
}
