//! Audio frame types

/// Sample format for audio data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SampleFormat {
    /// 32-bit float (most common for audio processing)
    Float32 = 0,
    /// 16-bit signed integer
    Int16 = 1,
    /// 32-bit signed integer
    Int32 = 2,
}

impl Default for SampleFormat {
    fn default() -> Self {
        SampleFormat::Float32
    }
}

/// Decoded audio frame
#[derive(Clone)]
pub struct AudioFrame {
    /// Interleaved audio sample data
    /// Format: [L0, R0, L1, R1, ...] for stereo
    pub data: Vec<f32>,

    /// Number of samples per channel
    pub sample_count: u32,

    /// Number of audio channels
    pub channels: u32,

    /// Sample rate in Hz
    pub sample_rate: u32,

    /// Presentation timestamp in microseconds
    pub pts_us: i64,

    /// Frame duration in microseconds
    pub duration_us: i64,

    /// Sequential frame number
    pub frame_number: i64,
}

impl AudioFrame {
    /// Create a new audio frame
    pub fn new(
        data: Vec<f32>,
        sample_count: u32,
        channels: u32,
        sample_rate: u32,
        pts_us: i64,
        duration_us: i64,
        frame_number: i64,
    ) -> Self {
        Self {
            data,
            sample_count,
            channels,
            sample_rate,
            pts_us,
            duration_us,
            frame_number,
        }
    }

    /// Get data size in bytes
    pub fn data_size(&self) -> usize {
        self.data.len() * std::mem::size_of::<f32>()
    }

    /// Get data pointer
    pub fn data_ptr(&self) -> *const f32 {
        self.data.as_ptr()
    }

    /// Get the number of total samples (sample_count * channels)
    pub fn total_samples(&self) -> usize {
        self.data.len()
    }

    /// Presentation time in seconds
    pub fn pts_seconds(&self) -> f64 {
        self.pts_us as f64 / 1_000_000.0
    }

    /// Duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.duration_us as f64 / 1_000_000.0
    }

    /// Calculate expected duration from sample count and rate
    pub fn calculate_duration_us(sample_count: u32, sample_rate: u32) -> i64 {
        if sample_rate == 0 {
            return 0;
        }
        (sample_count as i64 * 1_000_000) / sample_rate as i64
    }

    /// Create a test frame (for testing only)
    #[cfg(test)]
    pub fn test_frame(pts_us: i64, sample_count: u32, channels: u32, sample_rate: u32) -> Self {
        let total_samples = (sample_count * channels) as usize;
        let duration_us = Self::calculate_duration_us(sample_count, sample_rate);
        Self {
            data: vec![0.0f32; total_samples],
            sample_count,
            channels,
            sample_rate,
            pts_us,
            duration_us,
            frame_number: 0,
        }
    }
}

impl std::fmt::Debug for AudioFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioFrame")
            .field("sample_count", &self.sample_count)
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("pts_us", &self.pts_us)
            .field("duration_us", &self.duration_us)
            .field("frame_number", &self.frame_number)
            .field("data_len", &self.data.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_creation() {
        let frame = AudioFrame::test_frame(0, 1024, 2, 48000);
        assert_eq!(frame.sample_count, 1024);
        assert_eq!(frame.channels, 2);
        assert_eq!(frame.sample_rate, 48000);
        assert_eq!(frame.data.len(), 2048); // 1024 * 2 channels
    }

    #[test]
    fn test_duration_calculation() {
        // 1024 samples at 48000 Hz = 21.333... ms
        let duration = AudioFrame::calculate_duration_us(1024, 48000);
        assert_eq!(duration, 21333); // 21.333 ms
    }

    #[test]
    fn test_data_size() {
        let frame = AudioFrame::test_frame(0, 1024, 2, 48000);
        // 2048 samples * 4 bytes per f32 = 8192 bytes
        assert_eq!(frame.data_size(), 8192);
    }
}
