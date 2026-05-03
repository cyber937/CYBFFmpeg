//! Video frame types

use super::config::PixelFormat;

// CoreFoundation FFI shared with `super::ffmpeg_decoder` (which uses
// `CFRetain` on the HW-decode path). Declared here so the `Clone`/`Drop`
// impls below can balance the retain count for `cv_pixel_buffer_ptr`.
#[cfg(target_os = "macos")]
extern "C" {
    pub(super) fn CFRetain(cf: *const std::ffi::c_void) -> *const std::ffi::c_void;
    pub(super) fn CFRelease(cf: *const std::ffi::c_void);
}

/// Decoded video frame.
///
/// `cv_pixel_buffer_ptr` (when non-zero) is a CVPixelBufferRef held with one
/// reference owed to this VideoFrame. The custom `Clone`/`Drop` impls below
/// keep that refcount accurate across the cache (which clones into L1/L2/L3)
/// and the FFI hand-off (which moves the original to Swift). Without these,
/// auto-derived `Clone` would copy the bare u64 without `CFRetain`, leaving
/// the cached clones as dangling pointers once Swift releases its retain — a
/// pre-existing UAF that surfaces as `objc_retain` crashes during scrubbing.
pub struct VideoFrame {
    /// Raw pixel data (empty when `cv_pixel_buffer_ptr != 0`)
    pub data: Vec<u8>,

    /// Frame width
    pub width: u32,

    /// Frame height
    pub height: u32,

    /// Bytes per row (stride)
    pub stride: u32,

    /// Presentation timestamp in microseconds
    pub pts_us: i64,

    /// Frame duration in microseconds
    pub duration_us: i64,

    /// Whether this is a keyframe
    pub is_keyframe: bool,

    /// Sequential frame number
    pub frame_number: i64,

    /// Pixel format
    pub pixel_format: PixelFormat,

    /// CVPixelBufferRef pointer for VideoToolbox HW-decoded frames.
    /// Non-zero means the decoder produced a CVPixelBuffer directly (no
    /// SWS scaling, no Vec copy). Caller takes ownership of one CFRetain
    /// — must release exactly once on the Swift side.
    pub cv_pixel_buffer_ptr: u64,
}

impl VideoFrame {
    /// Create a new video frame
    pub fn new(
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
        pts_us: i64,
        duration_us: i64,
        is_keyframe: bool,
        frame_number: i64,
        pixel_format: PixelFormat,
    ) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            pts_us,
            duration_us,
            is_keyframe,
            frame_number,
            pixel_format,
            cv_pixel_buffer_ptr: 0,
        }
    }

    /// Create a HW-decoded frame backed by a CVPixelBuffer. Caller must
    /// have already CFRetain'd the buffer once on behalf of the consumer
    /// (the Swift bridge takes ownership of that retain).
    pub fn new_hw(
        cv_pixel_buffer_ptr: u64,
        width: u32,
        height: u32,
        pts_us: i64,
        duration_us: i64,
        is_keyframe: bool,
        frame_number: i64,
    ) -> Self {
        Self {
            data: Vec::new(),
            width,
            height,
            stride: 0,
            pts_us,
            duration_us,
            is_keyframe,
            frame_number,
            pixel_format: PixelFormat::Nv12,
            cv_pixel_buffer_ptr,
        }
    }

    /// Get data size in bytes
    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    /// Get data pointer
    pub fn data_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    /// Get mutable data pointer
    pub fn data_ptr_mut(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    /// Presentation time in seconds
    pub fn pts_seconds(&self) -> f64 {
        self.pts_us as f64 / 1_000_000.0
    }

    /// Duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.duration_us as f64 / 1_000_000.0
    }

    /// Calculate expected data size for format
    pub fn expected_size(width: u32, height: u32, format: PixelFormat) -> usize {
        match format {
            PixelFormat::Bgra => (width * height * 4) as usize,
            PixelFormat::Nv12 => (width * height * 3 / 2) as usize,
            PixelFormat::Yuv420p => (width * height * 3 / 2) as usize,
        }
    }

    /// Create a test frame (for testing only)
    #[cfg(test)]
    pub fn test_frame(pts_us: i64, width: u32, height: u32) -> Self {
        let size = Self::expected_size(width, height, PixelFormat::Bgra);
        Self {
            data: vec![0u8; size],
            width,
            height,
            stride: width * 4,
            pts_us,
            duration_us: 16666, // ~60fps
            is_keyframe: pts_us == 0,
            frame_number: pts_us / 16666,
            pixel_format: PixelFormat::Bgra,
            cv_pixel_buffer_ptr: 0,
        }
    }
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pts_us", &self.pts_us)
            .field("is_keyframe", &self.is_keyframe)
            .field("frame_number", &self.frame_number)
            .field("data_size", &self.data.len())
            .finish()
    }
}

impl Clone for VideoFrame {
    fn clone(&self) -> Self {
        #[cfg(target_os = "macos")]
        if self.cv_pixel_buffer_ptr != 0 {
            unsafe {
                CFRetain(self.cv_pixel_buffer_ptr as *const std::ffi::c_void);
            }
        }
        Self {
            data: self.data.clone(),
            width: self.width,
            height: self.height,
            stride: self.stride,
            pts_us: self.pts_us,
            duration_us: self.duration_us,
            is_keyframe: self.is_keyframe,
            frame_number: self.frame_number,
            pixel_format: self.pixel_format,
            cv_pixel_buffer_ptr: self.cv_pixel_buffer_ptr,
        }
    }
}

impl Drop for VideoFrame {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        if self.cv_pixel_buffer_ptr != 0 {
            unsafe {
                CFRelease(self.cv_pixel_buffer_ptr as *const std::ffi::c_void);
            }
            self.cv_pixel_buffer_ptr = 0;
        }
    }
}

// SAFETY: CVPixelBufferRef is thread-safe per Apple's CoreVideo docs — the
// underlying buffer can be passed across threads, and CFRetain/CFRelease are
// atomic. The cache holds VideoFrame across thread boundaries (decoder thread
// → main thread), so we need both Send and Sync. The raw pointer field is the
// only reason auto-derive doesn't grant these.
unsafe impl Send for VideoFrame {}
unsafe impl Sync for VideoFrame {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_creation() {
        let frame = VideoFrame::test_frame(0, 1920, 1080);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert!(frame.is_keyframe);
    }

    #[test]
    fn test_expected_size() {
        // 1080p BGRA = 1920 * 1080 * 4 = 8,294,400 bytes
        let size = VideoFrame::expected_size(1920, 1080, PixelFormat::Bgra);
        assert_eq!(size, 8_294_400);

        // 1080p NV12 = 1920 * 1080 * 1.5 = 3,110,400 bytes
        let size = VideoFrame::expected_size(1920, 1080, PixelFormat::Nv12);
        assert_eq!(size, 3_110_400);
    }
}
