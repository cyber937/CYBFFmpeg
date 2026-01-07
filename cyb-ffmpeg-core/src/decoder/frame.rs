//! Video frame types

use super::config::PixelFormat;

/// Decoded video frame
#[derive(Clone)]
pub struct VideoFrame {
    /// Raw pixel data
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
