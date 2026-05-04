//! Media information types

/// Exact frame rate as a rational number (`num / den`).
///
/// Used to represent any SMPTE rate losslessly:
/// `24000/1001` (23.976), `30000/1001` (29.97), `60000/1001` (59.94),
/// `24/1`, `25/1`, `30/1`, `50/1`, `60/1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    /// Numerator (e.g., 24000)
    pub num: u32,
    /// Denominator (e.g., 1001)
    pub den: u32,
}

impl FrameRate {
    /// Create a new FrameRate. Returns None if `den == 0`.
    pub fn new(num: u32, den: u32) -> Option<Self> {
        if den == 0 {
            None
        } else {
            Some(Self { num, den })
        }
    }

    /// Approximate `Double` representation. Lossy for NTSC rates.
    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Whether this is an NTSC rate (denominator 1001).
    pub fn is_ntsc(self) -> bool {
        self.den == 1001
    }

    /// Integer "TC rate" used by SMPTE for clock arithmetic.
    /// 23.976 → 24, 29.97 → 30, 59.94 → 60, 25 → 25, 30 → 30, 60 → 60.
    pub fn tc_integer_rate(self) -> u32 {
        if self.is_ntsc() {
            // 24000/1001 → 24, 30000/1001 → 30, 60000/1001 → 60
            // (num is divisible by 1000 for the standard NTSC rates)
            self.num / 1000
        } else if self.den == 0 {
            30
        } else {
            self.num / self.den
        }
    }
}

/// Provenance of a timecode reading.
///
/// `confidence` should mirror this:
/// - `TmcdTrack` / `MxfMaterialPackage`: ~0.95
/// - `ContainerMetadata`: ~0.85
/// - `MxfSourcePackage`: ~0.80 (secondary track)
/// - `Inferred`: ~0.30 (frame rate known, TC assumed 0)
/// - `Unknown`: 0.0
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TimecodeSourceKind {
    /// `tmcd` track in QuickTime / MP4
    TmcdTrack = 0,
    /// MXF Material Package timecode component (primary editing TC)
    MxfMaterialPackage = 1,
    /// MXF Source Package timecode (secondary, original capture TC)
    MxfSourcePackage = 2,
    /// Container-level metadata `timecode` tag (FFmpeg `timecode` dict entry)
    ContainerMetadata = 3,
    /// Frame rate known but no embedded TC found; assumed `00:00:00:00`
    Inferred = 4,
    /// Cannot be determined
    Unknown = 5,
}

impl TimecodeSourceKind {
    /// Convert to a stable string representation for cross-layer transport.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TmcdTrack => "tmcd",
            Self::MxfMaterialPackage => "mxf-material",
            Self::MxfSourcePackage => "mxf-source",
            Self::ContainerMetadata => "metadata",
            Self::Inferred => "inferred",
            Self::Unknown => "unknown",
        }
    }
}

/// Canonical SMPTE timecode.
///
/// `frame_number` is the absolute count of frames from `00:00:00:00`,
/// computed using the SMPTE 12M algorithm (drop-frame skips are handled
/// when `drop_frame == true`). It is the only frame-precise representation;
/// `as_smpte_string` and `as_seconds` are derivations.
#[derive(Debug, Clone, PartialEq)]
pub struct Timecode {
    /// Absolute frame count from `00:00:00:00`. Always counts real frames
    /// (drop-frame label skips do not change this number).
    pub frame_number: i64,
    /// Frame rate as a rational
    pub rate: FrameRate,
    /// Whether the source uses drop-frame timecode (NTSC family only)
    pub drop_frame: bool,
    /// Where this TC was read from
    pub source_kind: TimecodeSourceKind,
    /// Free-form description of the source (e.g. "stream 0 timecode tag",
    /// "tmcd track index 2"). May be empty.
    pub source_detail: String,
    /// Confidence in the reading, 0.0–1.0.
    pub confidence: f32,
}

impl Timecode {
    /// Format the timecode as a SMPTE string. Drop-frame uses `;` between
    /// seconds and frames; non-drop uses `:`. Hours are zero-padded to 2.
    pub fn as_smpte_string(&self) -> String {
        let (h, m, s, f) = frames_to_smpte_components(
            self.frame_number,
            self.rate.tc_integer_rate(),
            self.drop_frame,
        );
        let sep = if self.drop_frame { ';' } else { ':' };
        format!("{:02}:{:02}:{:02}{}{:02}", h, m, s, sep, f)
    }

    /// Approximate seconds (lossy — for UI / sorting only, not canonical).
    pub fn as_seconds(&self) -> f64 {
        self.frame_number as f64 * self.rate.den as f64 / self.rate.num as f64
    }
}

/// Convert an absolute frame count into SMPTE (h, m, s, f) components.
///
/// `tc_int_rate` is the integer "label rate" — 24 for 23.976, 30 for 29.97,
/// 60 for 59.94, etc.
///
/// Reference: SMPTE 12M-1.
pub fn frames_to_smpte_components(
    frame_number: i64,
    tc_int_rate: u32,
    drop_frame: bool,
) -> (u32, u32, u32, u32) {
    if frame_number < 0 {
        return (0, 0, 0, 0);
    }
    let n = tc_int_rate as i64;
    if !drop_frame || n < 30 {
        // Non-drop-frame, or rates that don't support drop (24, 25)
        let total_seconds = frame_number / n;
        let f = (frame_number % n) as u32;
        let h = (total_seconds / 3600) as u32;
        let m = ((total_seconds / 60) % 60) as u32;
        let s = (total_seconds % 60) as u32;
        return (h, m, s, f);
    }
    // Drop-frame: 30 (29.97) drops 2 per minute, 60 (59.94) drops 4 per minute.
    let drops_per_minute: i64 = if n == 60 { 4 } else { 2 };
    let frames_per_minute = n * 60 - drops_per_minute; // 1798 / 3596
    let frames_per_10_minutes = n * 60 * 10 - drops_per_minute * 9; // 17982 / 35964

    let d = frame_number / frames_per_10_minutes;
    let m_in_block = frame_number % frames_per_10_minutes;

    // Map back to non-drop frame index by adding back drops we'd skip
    let frame_count = if m_in_block > drops_per_minute {
        frame_number
            + drops_per_minute * 9 * d
            + drops_per_minute * ((m_in_block - drops_per_minute) / frames_per_minute)
    } else {
        frame_number + drops_per_minute * 9 * d
    };

    let f = (frame_count % n) as u32;
    let s = ((frame_count / n) % 60) as u32;
    let m = ((frame_count / (n * 60)) % 60) as u32;
    let h = (frame_count / (n * 3600)) as u32;
    (h, m, s, f)
}

/// Convert SMPTE (h, m, s, f) components to an absolute frame count.
pub fn smpte_components_to_frames(
    h: u32,
    m: u32,
    s: u32,
    f: u32,
    tc_int_rate: u32,
    drop_frame: bool,
) -> i64 {
    let n = tc_int_rate as i64;
    let total_minutes = (h as i64) * 60 + m as i64;
    let raw =
        ((h as i64) * 3600 + (m as i64) * 60 + s as i64) * n + f as i64;
    if !drop_frame || n < 30 {
        raw
    } else {
        let drops_per_minute: i64 = if n == 60 { 4 } else { 2 };
        raw - drops_per_minute * (total_minutes - total_minutes / 10)
    }
}

/// Parse a SMPTE timecode string ("HH:MM:SS:FF" or "HH:MM:SS;FF") into an
/// absolute frame count. The `:` vs `;` separator implies drop-frame intent
/// when `drop_frame_hint` is `None`; pass `Some(true)`/`Some(false)` to override.
///
/// Returns `None` for malformed input. Never panics.
pub fn parse_smpte_string(
    s: &str,
    rate: FrameRate,
    drop_frame_hint: Option<bool>,
) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || s.len() < 11 {
        // Minimum valid is "0:00:00:00" — but be strict, require "HH:MM:SS:FF"
        return None;
    }

    // Detect drop frame from separator
    let bytes = s.as_bytes();
    let last_sep_idx = s.len() - 3; // separator before the FF
    let last_sep = bytes.get(last_sep_idx).copied()? as char;
    let from_sep = match last_sep {
        ';' => Some(true),
        ':' | '.' => Some(false),
        _ => None,
    };
    let drop_frame = drop_frame_hint.or(from_sep).unwrap_or(false);

    // Split on any of `:`, `;`, `.`
    let parts: Vec<&str> = s.split(|c: char| c == ':' || c == ';' || c == '.').collect();
    if parts.len() != 4 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let sec: u32 = parts[2].parse().ok()?;
    let f: u32 = parts[3].parse().ok()?;

    // Validate field ranges
    let tc_rate = rate.tc_integer_rate();
    if tc_rate == 0 || m >= 60 || sec >= 60 || f >= tc_rate {
        return None;
    }

    Some(smpte_components_to_frames(h, m, sec, f, tc_rate, drop_frame))
}

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

    /// Exact frame rate (rational). `None` if not derivable from the stream.
    /// Prefer this over `frame_rate` (Double) for SMPTE arithmetic.
    pub frame_rate_exact: Option<FrameRate>,

    /// Embedded SMPTE start timecode for this track. `None` if no TC was found.
    pub start_timecode: Option<Timecode>,
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
            frame_rate_exact: None,
            start_timecode: None,
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

    // -- FrameRate -----------------------------------------------------------

    #[test]
    fn frame_rate_new_rejects_zero_denominator() {
        assert!(FrameRate::new(24, 0).is_none());
        assert!(FrameRate::new(24, 1).is_some());
    }

    #[test]
    fn frame_rate_tc_integer_rate() {
        // NTSC family
        assert_eq!(FrameRate::new(24000, 1001).unwrap().tc_integer_rate(), 24);
        assert_eq!(FrameRate::new(30000, 1001).unwrap().tc_integer_rate(), 30);
        assert_eq!(FrameRate::new(60000, 1001).unwrap().tc_integer_rate(), 60);
        // Integer rates
        assert_eq!(FrameRate::new(24, 1).unwrap().tc_integer_rate(), 24);
        assert_eq!(FrameRate::new(25, 1).unwrap().tc_integer_rate(), 25);
        assert_eq!(FrameRate::new(30, 1).unwrap().tc_integer_rate(), 30);
        assert_eq!(FrameRate::new(50, 1).unwrap().tc_integer_rate(), 50);
        assert_eq!(FrameRate::new(60, 1).unwrap().tc_integer_rate(), 60);
    }

    #[test]
    fn frame_rate_is_ntsc() {
        assert!(FrameRate::new(24000, 1001).unwrap().is_ntsc());
        assert!(FrameRate::new(30000, 1001).unwrap().is_ntsc());
        assert!(!FrameRate::new(24, 1).unwrap().is_ntsc());
        assert!(!FrameRate::new(25, 1).unwrap().is_ntsc());
    }

    // -- SMPTE round-trip: NDF ----------------------------------------------

    #[test]
    fn smpte_round_trip_24fps_ndf() {
        // 24 fps non-drop, "01:00:00:00" = 24 * 3600 = 86400
        let frames = smpte_components_to_frames(1, 0, 0, 0, 24, false);
        assert_eq!(frames, 86400);
        let (h, m, s, f) = frames_to_smpte_components(86400, 24, false);
        assert_eq!((h, m, s, f), (1, 0, 0, 0));
    }

    #[test]
    fn smpte_round_trip_23976_ndf_uses_24_label() {
        // 23.976 NDF uses 24 as label rate (TC counts as if 24fps).
        // "12:34:56:00" → frames = (12*3600 + 34*60 + 56) * 24 = 1087104
        let rate = FrameRate::new(24000, 1001).unwrap();
        let tc_rate = rate.tc_integer_rate();
        let frames = smpte_components_to_frames(12, 34, 56, 0, tc_rate, false);
        assert_eq!(frames, 1087104);
        let (h, m, s, f) = frames_to_smpte_components(frames, tc_rate, false);
        assert_eq!((h, m, s, f), (12, 34, 56, 0));
    }

    #[test]
    fn smpte_round_trip_25fps_ndf() {
        // PAL 25 fps, "00:01:00:00" = 25 * 60 = 1500
        let frames = smpte_components_to_frames(0, 1, 0, 0, 25, false);
        assert_eq!(frames, 1500);
        let (h, m, s, f) = frames_to_smpte_components(1500, 25, false);
        assert_eq!((h, m, s, f), (0, 1, 0, 0));
    }

    #[test]
    fn smpte_round_trip_30fps_ndf() {
        // 30 fps, "01:00:00:00" = 30 * 3600 = 108000
        let frames = smpte_components_to_frames(1, 0, 0, 0, 30, false);
        assert_eq!(frames, 108000);
        let (h, m, s, f) = frames_to_smpte_components(108000, 30, false);
        assert_eq!((h, m, s, f), (1, 0, 0, 0));
    }

    // -- SMPTE round-trip: DF ------------------------------------------------

    #[test]
    fn smpte_2997_df_one_hour_is_107892_frames() {
        // SMPTE 12M reference: 29.97 DF, "01:00:00:00" = 107892 frames.
        let frames = smpte_components_to_frames(1, 0, 0, 0, 30, true);
        assert_eq!(frames, 107892);
        let (h, m, s, f) = frames_to_smpte_components(107892, 30, true);
        assert_eq!((h, m, s, f), (1, 0, 0, 0));
    }

    #[test]
    fn smpte_2997_df_minute_one_drops_two_frames() {
        // 29.97 DF: at 1-minute boundary, frames 00 and 01 are skipped.
        // Frame after 00:00:59:29 (= 1799) should be labelled "00:01:00;02".
        let frames_at_minute = smpte_components_to_frames(0, 1, 0, 2, 30, true);
        assert_eq!(frames_at_minute, 1800);
        let (h, m, s, f) = frames_to_smpte_components(1800, 30, true);
        assert_eq!((h, m, s, f), (0, 1, 0, 2));
    }

    #[test]
    fn smpte_2997_df_tenth_minute_no_drop() {
        // 29.97 DF: every 10th minute does NOT drop. Frame at "00:10:00;00"
        // = 9*1798 + 1*1800 = 16182 + 1800 = 17982
        let frames = smpte_components_to_frames(0, 10, 0, 0, 30, true);
        assert_eq!(frames, 17982);
        let (h, m, s, f) = frames_to_smpte_components(17982, 30, true);
        assert_eq!((h, m, s, f), (0, 10, 0, 0));
    }

    #[test]
    fn smpte_5994_df_one_hour() {
        // 59.94 DF: 60fps with 4 dropped per minute (except every 10th).
        // 1 hour = 60*3600 - 4 * (60 - 6) = 216000 - 216 = 215784.
        let frames = smpte_components_to_frames(1, 0, 0, 0, 60, true);
        assert_eq!(frames, 215784);
        let (h, m, s, f) = frames_to_smpte_components(215784, 60, true);
        assert_eq!((h, m, s, f), (1, 0, 0, 0));
    }

    // -- parse_smpte_string ---------------------------------------------------

    #[test]
    fn parse_smpte_string_ndf_basic() {
        let rate = FrameRate::new(24, 1).unwrap();
        assert_eq!(parse_smpte_string("01:00:00:00", rate, None), Some(86400));
        assert_eq!(parse_smpte_string("00:00:00:00", rate, None), Some(0));
        assert_eq!(parse_smpte_string("00:00:01:00", rate, None), Some(24));
        assert_eq!(parse_smpte_string("00:00:00:23", rate, None), Some(23));
    }

    #[test]
    fn parse_smpte_string_df_via_semicolon() {
        // Semicolon implies drop-frame.
        let rate = FrameRate::new(30000, 1001).unwrap();
        assert_eq!(parse_smpte_string("01:00:00;00", rate, None), Some(107892));
    }

    #[test]
    fn parse_smpte_string_df_via_explicit_hint() {
        // Colon-separated string but explicit DF hint.
        let rate = FrameRate::new(30000, 1001).unwrap();
        assert_eq!(
            parse_smpte_string("01:00:00:00", rate, Some(true)),
            Some(107892)
        );
    }

    #[test]
    fn parse_smpte_string_rejects_malformed() {
        let rate = FrameRate::new(24, 1).unwrap();
        assert_eq!(parse_smpte_string("", rate, None), None);
        assert_eq!(parse_smpte_string("garbage", rate, None), None);
        assert_eq!(parse_smpte_string("1:2:3", rate, None), None); // wrong shape
        assert_eq!(parse_smpte_string("01:00:00:99", rate, None), None); // f >= rate
        assert_eq!(parse_smpte_string("01:60:00:00", rate, None), None); // m >= 60
        assert_eq!(parse_smpte_string("01:00:60:00", rate, None), None); // s >= 60
    }

    #[test]
    fn parse_smpte_string_round_trip_via_timecode() {
        let rate = FrameRate::new(24000, 1001).unwrap();
        let frames = parse_smpte_string("12:34:56:00", rate, Some(false)).unwrap();
        let tc = Timecode {
            frame_number: frames,
            rate,
            drop_frame: false,
            source_kind: TimecodeSourceKind::ContainerMetadata,
            source_detail: String::new(),
            confidence: 0.85,
        };
        assert_eq!(tc.as_smpte_string(), "12:34:56:00");
    }

    #[test]
    fn timecode_as_smpte_string_uses_semicolon_for_df() {
        let rate = FrameRate::new(30000, 1001).unwrap();
        let tc = Timecode {
            frame_number: 1800, // 00:01:00;02 in 29.97 DF
            rate,
            drop_frame: true,
            source_kind: TimecodeSourceKind::TmcdTrack,
            source_detail: String::new(),
            confidence: 0.95,
        };
        assert_eq!(tc.as_smpte_string(), "00:01:00;02");
    }

    #[test]
    fn timecode_as_seconds_lossy() {
        // 23.976 NDF, "01:00:00:00" = 86400 frames = 86400 * 1001/24000 ≈ 3603.6 s
        let rate = FrameRate::new(24000, 1001).unwrap();
        let tc = Timecode {
            frame_number: 86400,
            rate,
            drop_frame: false,
            source_kind: TimecodeSourceKind::TmcdTrack,
            source_detail: String::new(),
            confidence: 0.95,
        };
        let secs = tc.as_seconds();
        assert!((secs - 3603.6).abs() < 0.001, "got {}", secs);
    }
}
