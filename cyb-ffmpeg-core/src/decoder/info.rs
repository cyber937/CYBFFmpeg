//! Media information types

/// Codec information
#[derive(Debug, Clone)]
pub struct CodecInfo {
    /// Short name (e.g., "vp9")
    pub name: String,

    /// Long name (e.g., "Google VP9")
    pub long_name: String,

    /// FourCC code (e.g., "vp09")
    pub four_cc: Option<String>,
}

impl CodecInfo {
    /// Create unknown codec info
    pub fn unknown() -> Self {
        Self {
            name: "unknown".to_string(),
            long_name: "Unknown Codec".to_string(),
            four_cc: None,
        }
    }
}

/// Video track information
#[derive(Debug, Clone)]
pub struct VideoTrack {
    /// Track index
    pub index: i32,

    /// Codec info
    pub codec: CodecInfo,

    /// Width in pixels
    pub width: i32,

    /// Height in pixels
    pub height: i32,

    /// Frame rate
    pub frame_rate: f64,

    /// Bit rate in bps
    pub bit_rate: i64,

    /// Pixel format string
    pub pixel_format: String,

    /// Whether VideoToolbox can decode this
    pub is_hardware_decodable: bool,

    /// Color space
    pub color_space: Option<String>,

    /// Color primaries
    pub color_primaries: Option<String>,

    /// Color transfer function
    pub color_transfer: Option<String>,

    /// Color range
    pub color_range: String,
}

impl VideoTrack {
    /// Create a placeholder track
    pub fn placeholder() -> Self {
        Self {
            index: 0,
            codec: CodecInfo::unknown(),
            width: 1920,
            height: 1080,
            frame_rate: 24.0,
            bit_rate: 0,
            pixel_format: "yuv420p".to_string(),
            is_hardware_decodable: false,
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            color_range: "unknown".to_string(),
        }
    }
}

/// Audio track information
#[derive(Debug, Clone)]
pub struct AudioTrack {
    /// Track index
    pub index: i32,

    /// Codec info
    pub codec: CodecInfo,

    /// Sample rate in Hz
    pub sample_rate: i32,

    /// Number of channels
    pub channels: i32,

    /// Channel layout string
    pub channel_layout: Option<String>,

    /// Bit rate in bps
    pub bit_rate: i64,

    /// Language code
    pub language_code: Option<String>,
}

impl AudioTrack {
    /// Create a placeholder track
    pub fn placeholder() -> Self {
        Self {
            index: 0,
            codec: CodecInfo::unknown(),
            sample_rate: 48000,
            channels: 2,
            channel_layout: Some("stereo".to_string()),
            bit_rate: 0,
            language_code: None,
        }
    }
}

/// Complete media information
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// Duration in seconds
    pub duration: f64,

    /// Container format
    pub container_format: String,

    /// Video tracks
    pub video_tracks: Vec<VideoTrack>,

    /// Audio tracks
    pub audio_tracks: Vec<AudioTrack>,

    /// Metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl MediaInfo {
    /// Create placeholder media info
    pub fn placeholder(path: &str) -> Self {
        log::debug!("Creating placeholder media info for: {}", path);

        Self {
            duration: 0.0,
            container_format: "unknown".to_string(),
            video_tracks: vec![VideoTrack::placeholder()],
            audio_tracks: vec![AudioTrack::placeholder()],
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Check if media has video
    pub fn has_video(&self) -> bool {
        !self.video_tracks.is_empty()
    }

    /// Check if media has audio
    pub fn has_audio(&self) -> bool {
        !self.audio_tracks.is_empty()
    }

    /// Get primary video track
    pub fn primary_video(&self) -> Option<&VideoTrack> {
        self.video_tracks.first()
    }

    /// Get primary audio track
    pub fn primary_audio(&self) -> Option<&AudioTrack> {
        self.audio_tracks.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_info() {
        let info = MediaInfo::placeholder("/test/video.mp4");
        assert!(info.has_video());
        assert!(info.has_audio());
    }

    #[test]
    fn test_primary_tracks() {
        let info = MediaInfo::placeholder("/test/video.mp4");
        assert!(info.primary_video().is_some());
        assert!(info.primary_audio().is_some());
    }
}
