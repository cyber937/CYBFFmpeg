//! FFmpeg decoder implementation using ffmpeg-next
//!
//! This module provides the actual FFmpeg integration via ffmpeg-next bindings.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use ffmpeg_next as ffmpeg;
use ffmpeg_next::codec::context::Context as CodecContext;
use ffmpeg_next::format::context::Input as FormatContext;
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::software::resampling::Context as ResamplerContext;
use ffmpeg_next::software::scaling::{Context as ScalerContext, Flags as ScalerFlags};
use ffmpeg_next::util::frame::audio::Audio as AudioFrameFFmpeg;
use ffmpeg_next::util::frame::video::Video as VideoFrameFFmpeg;
use ffmpeg_next::Rational;

use super::audio_frame::AudioFrame;
use super::config::{DecoderConfig, PixelFormat};
use super::frame::VideoFrame;
use super::info::{AudioTrack, CodecInfo, MediaInfo, VideoTrack};
use crate::error::{Error, Result};

/// FFmpeg decoder context
pub struct FFmpegContext {
    /// Input format context
    input: FormatContext,

    /// Video stream index
    video_stream_index: Option<usize>,

    /// Audio stream index
    audio_stream_index: Option<usize>,

    /// Video decoder
    video_decoder: Option<ffmpeg::decoder::Video>,

    /// Audio decoder
    audio_decoder: Option<ffmpeg::decoder::Audio>,

    /// Scaler for pixel format conversion
    scaler: Option<ScalerContext>,

    /// Resampler for audio format conversion
    resampler: Option<ResamplerContext>,

    /// Target pixel format
    target_format: PixelFormat,

    /// Target audio sample rate (default: 48000 Hz)
    target_sample_rate: u32,

    /// Target audio channels (default: 2 = stereo)
    target_channels: u32,

    /// Frame counter
    frame_number: i64,

    /// Audio frame counter
    audio_frame_number: i64,

    /// Time base for video stream
    video_time_base: Rational,

    /// Time base for audio stream
    audio_time_base: Rational,

    /// Video duration in microseconds
    duration_us: i64,

    /// Frame rate
    frame_rate: f64,

    /// Video width
    width: u32,

    /// Video height
    height: u32,

    /// Audio sample rate (source)
    audio_sample_rate: u32,

    /// Audio channels (source)
    audio_channels: u32,

    /// Prefer hardware decoding
    prefer_hw: bool,

    /// Queue of audio packets collected during video decoding
    audio_packet_queue: VecDeque<ffmpeg::Packet>,

    /// Queue of video packets collected during audio decoding
    video_packet_queue: VecDeque<ffmpeg::Packet>,
}

impl FFmpegContext {
    /// Create a new FFmpeg context
    pub fn new<P: AsRef<Path>>(path: P, config: &DecoderConfig) -> Result<Self> {
        let path_ref = path.as_ref();

        // Initialize FFmpeg (safe to call multiple times)
        ffmpeg::init().map_err(|e| Error::FFmpeg {
            code: -1,
            message: format!("FFmpeg init failed: {}", e),
        })?;

        // Open input file
        let input = ffmpeg::format::input(path_ref).map_err(|e| {
            if e.to_string().contains("No such file") {
                Error::FileNotFound(path_ref.to_path_buf())
            } else {
                Error::InvalidFormat(e.to_string())
            }
        })?;

        log::debug!("Opened file: {:?}", path_ref);

        // Find video stream
        let video_stream_index = input
            .streams()
            .best(MediaType::Video)
            .map(|s| s.index());

        // Find audio stream
        let audio_stream_index = input
            .streams()
            .best(MediaType::Audio)
            .map(|s| s.index());

        let mut ctx = Self {
            input,
            video_stream_index,
            audio_stream_index,
            video_decoder: None,
            audio_decoder: None,
            scaler: None,
            resampler: None,
            target_format: config.output_pixel_format,
            target_sample_rate: 48000, // Standard audio sample rate
            target_channels: 2,        // Stereo
            frame_number: 0,
            audio_frame_number: 0,
            video_time_base: Rational::new(1, 1000000),
            audio_time_base: Rational::new(1, 1000000),
            duration_us: 0,
            frame_rate: 0.0,
            width: 0,
            height: 0,
            audio_sample_rate: 0,
            audio_channels: 0,
            prefer_hw: config.prefer_hardware_decoding,
            audio_packet_queue: VecDeque::with_capacity(64),
            video_packet_queue: VecDeque::with_capacity(32),
        };

        // Initialize video decoder if we have a video stream
        if let Some(stream_idx) = ctx.video_stream_index {
            ctx.init_video_decoder(stream_idx, config)?;
        }

        // Initialize audio decoder if we have an audio stream
        if let Some(stream_idx) = ctx.audio_stream_index {
            ctx.init_audio_decoder(stream_idx)?;
        }

        Ok(ctx)
    }

    /// Initialize video decoder for a stream
    fn init_video_decoder(&mut self, stream_index: usize, config: &DecoderConfig) -> Result<()> {
        let stream = self.input.stream(stream_index).ok_or_else(|| {
            Error::InvalidFormat(format!("Video stream {} not found", stream_index))
        })?;

        // Get codec parameters
        let codec_params = stream.parameters();
        self.video_time_base = stream.time_base();

        // Calculate duration
        // For elementary streams like .m2v, neither stream nor container duration
        // may be available. Try multiple sources in order of reliability.
        let stream_duration = stream.duration();
        let container_duration = self.input.duration();

        // AV_NOPTS_VALUE is 0x8000000000000000 (i64::MIN)
        // We use a special marker for "needs scan" that's different
        const NEEDS_SCAN_MARKER: i64 = -999_999_999_999;

        // Helper to get file size
        let get_file_size = || -> i64 {
            unsafe {
                let pb = (*self.input.as_ptr()).pb;
                if !pb.is_null() {
                    ffmpeg::ffi::avio_size(pb)
                } else {
                    0
                }
            }
        };

        // Check if duration value is valid (not AV_NOPTS_VALUE or other invalid values)
        // AV_NOPTS_VALUE = 0x8000000000000000 = i64::MIN = -9223372036854775808
        let is_valid_duration = |d: i64| -> bool {
            // Valid if positive and not near i64::MIN (AV_NOPTS_VALUE)
            d > 0 && d < i64::MAX / 2
        };

        log::debug!("Raw durations - stream: {}, container: {}, AV_NOPTS_VALUE: {}",
            stream_duration, container_duration, ffmpeg::ffi::AV_NOPTS_VALUE);

        // Calculate both durations and use the longer one
        // This handles cases where audio is longer than video, or vice versa
        let stream_duration_us = if is_valid_duration(stream_duration) {
            Self::pts_to_us(stream_duration, self.video_time_base)
        } else {
            0
        };
        let container_duration_us = if is_valid_duration(container_duration) {
            container_duration * 1_000_000 / ffmpeg::ffi::AV_TIME_BASE as i64
        } else {
            0
        };

        log::debug!("Calculated durations - stream: {} us, container: {} us",
            stream_duration_us, container_duration_us);

        if stream_duration_us > 0 || container_duration_us > 0 {
            // Use the longer of the two durations
            // This ensures we don't cut off audio if video is shorter, or vice versa
            self.duration_us = stream_duration_us.max(container_duration_us);
            log::debug!("Duration from {}: {} us ({:.2}s)",
                if self.duration_us == stream_duration_us { "stream" } else { "container" },
                self.duration_us, self.duration_us as f64 / 1_000_000.0);
        } else {
            // For elementary streams (.m2v, .m2a, etc.) with no duration info
            let bit_rate = unsafe { (*self.input.as_ptr()).bit_rate };
            let file_size = get_file_size();
            let nb_frames = stream.frames();

            log::debug!("Duration fallback: bit_rate={}, file_size={}, nb_frames={}",
                bit_rate, file_size, nb_frames);

            // Check if nb_frames is valid (not 0, not AV_NOPTS_VALUE-like values)
            // AV_NOPTS_VALUE is i64::MIN, but nb_frames could also be very large invalid values
            let is_valid_frame_count = |n: i64| -> bool {
                n > 0 && n < 100_000_000 // Max 100 million frames (~1000 hours at 30fps)
            };

            if bit_rate > 0 && file_size > 0 {
                // Estimate from file size and bitrate
                // duration = file_size * 8 / bit_rate (in seconds)
                self.duration_us = (file_size * 8 * 1_000_000) / bit_rate;
                log::info!("Duration estimated from file size ({} bytes) and bitrate ({} bps): {} us",
                    file_size, bit_rate, self.duration_us);
            } else if is_valid_frame_count(nb_frames) {
                // Will calculate duration after frame rate is known
                log::debug!("Stream has {} frames, will calculate duration after frame rate", nb_frames);
                // Store frame count temporarily, negative to indicate it's a frame count
                self.duration_us = -nb_frames;
            } else {
                // For elementary streams like .m2v, we need to scan the file to count frames
                // This is the only reliable way to get duration for these formats
                log::info!("Elementary stream detected, will scan for duration after decoder init");
                self.duration_us = NEEDS_SCAN_MARKER;
            }
        }

        // Get frame rate - need to choose between r_frame_rate and avg_frame_rate carefully
        // - r_frame_rate: For elementary streams (.m2v), this is accurate (e.g., 24000/1001)
        // - avg_frame_rate: For container formats (.mpeg), this is often more accurate
        // - r_frame_rate can be field rate (2x frame rate) for interlaced content in containers
        let r_frame_rate = stream.rate();
        let avg_frame_rate = stream.avg_frame_rate();

        let r_fps = if r_frame_rate.denominator() > 0 {
            r_frame_rate.numerator() as f64 / r_frame_rate.denominator() as f64
        } else {
            0.0
        };
        let avg_fps = if avg_frame_rate.denominator() > 0 {
            avg_frame_rate.numerator() as f64 / avg_frame_rate.denominator() as f64
        } else {
            0.0
        };

        log::debug!("Frame rates - r_frame_rate: {}/{} = {:.3} fps, avg_frame_rate: {}/{} = {:.3} fps",
            r_frame_rate.numerator(), r_frame_rate.denominator(), r_fps,
            avg_frame_rate.numerator(), avg_frame_rate.denominator(), avg_fps);

        let (frame_rate, rate_source) = if avg_fps > 0.0 && r_fps > 0.0 {
            // Both are valid - check if r_frame_rate is a field rate (2x avg)
            // For interlaced content, r_frame_rate can be field rate (48fps for 24fps content)
            if (r_fps / avg_fps - 2.0).abs() < 0.1 {
                // r_frame_rate is likely field rate, use avg_frame_rate
                log::debug!("r_frame_rate appears to be field rate (2x avg), using avg_frame_rate");
                (avg_frame_rate, "avg_frame_rate")
            } else if self.duration_us == NEEDS_SCAN_MARKER {
                // Elementary stream - prefer r_frame_rate
                (r_frame_rate, "r_frame_rate")
            } else {
                // Container format with valid duration - prefer avg_frame_rate
                (avg_frame_rate, "avg_frame_rate")
            }
        } else if r_fps > 0.0 {
            (r_frame_rate, "r_frame_rate")
        } else if avg_fps > 0.0 {
            (avg_frame_rate, "avg_frame_rate")
        } else {
            // Default to 24 fps
            self.frame_rate = 24.0;
            log::warn!("No valid frame rate found, using default 24.0 fps");
            (ffmpeg::Rational::new(24, 1), "default")
        };

        if rate_source != "default" {
            self.frame_rate = frame_rate.numerator() as f64 / frame_rate.denominator() as f64;
        }

        log::info!("Frame rate from {}: {}/{} = {:.6} fps",
            rate_source, frame_rate.numerator(), frame_rate.denominator(), self.frame_rate);

        // If duration_us is negative (but not the scan marker), it contains frame count
        if self.duration_us < 0 && self.duration_us != NEEDS_SCAN_MARKER {
            let nb_frames = -self.duration_us;
            if self.frame_rate > 0.0 {
                self.duration_us = ((nb_frames as f64 / self.frame_rate) * 1_000_000.0) as i64;
                log::info!("Duration calculated from {} frames at {:.2} fps: {} us ({:.2}s)",
                    nb_frames, self.frame_rate, self.duration_us, self.duration_us as f64 / 1_000_000.0);
            }
        }

        // Find decoder
        let decoder_codec = ffmpeg::decoder::find(codec_params.id()).ok_or_else(|| {
            Error::CodecNotSupported(format!("No decoder for codec: {:?}", codec_params.id()))
        })?;

        log::info!(
            "Using decoder: {} ({})",
            decoder_codec.name(),
            decoder_codec.description()
        );

        // Create decoder context
        let mut decoder_ctx = CodecContext::new_with_codec(decoder_codec);
        decoder_ctx.set_parameters(codec_params).map_err(|e| {
            Error::DecodeFailed(format!("Failed to set codec parameters: {}", e))
        })?;

        // Set threading options
        if config.thread_count > 0 {
            unsafe {
                (*decoder_ctx.as_mut_ptr()).thread_count = config.thread_count as i32;
            }
        }

        // Open decoder
        let mut video_decoder = decoder_ctx.decoder().video().map_err(|e| {
            Error::DecodeFailed(format!("Failed to open video decoder: {}", e))
        })?;

        self.width = video_decoder.width();
        self.height = video_decoder.height();

        // Check if we need to scan for duration (elementary stream marker)
        // NEEDS_SCAN_MARKER is defined earlier in this function
        if self.duration_us == NEEDS_SCAN_MARKER {
            // Elementary stream - need to scan entire file to count frames
            // For elementary streams like .m2v, PTS values reset after seek,
            // so we must count all frames from the beginning
            log::info!("Scanning elementary stream for duration (counting all frames)...");

            let mut frame_count: i64 = 0;
            let mut max_pts: i64 = 0;
            let scan_start = std::time::Instant::now();

            // Count all video packets from the beginning
            for (stream, packet) in self.input.packets() {
                if Some(stream.index()) == self.video_stream_index {
                    frame_count += 1;
                    // Track max PTS for verification
                    if let Some(pts) = packet.pts() {
                        if pts > max_pts {
                            max_pts = pts;
                        }
                    }
                    if let Some(dts) = packet.dts() {
                        if dts > max_pts {
                            max_pts = dts;
                        }
                    }
                }
            }

            let scan_duration = scan_start.elapsed();
            log::info!("Scanned {} frames in {:.2}s, max PTS: {}",
                frame_count, scan_duration.as_secs_f64(), max_pts);

            if frame_count > 0 && self.frame_rate > 0.0 {
                // Calculate duration from frame count and frame rate
                self.duration_us = ((frame_count as f64 / self.frame_rate) * 1_000_000.0) as i64;
                log::info!("Duration from frame count: {} frames / {:.2} fps = {} us ({:.2}s)",
                    frame_count, self.frame_rate, self.duration_us,
                    self.duration_us as f64 / 1_000_000.0);
            } else if max_pts > 0 {
                // Fallback to PTS-based calculation
                self.duration_us = Self::pts_to_us(max_pts, self.video_time_base);
                if self.frame_rate > 0.0 {
                    self.duration_us += (1_000_000.0 / self.frame_rate) as i64;
                }
                log::info!("Duration from max PTS: {} us ({:.2}s)",
                    self.duration_us, self.duration_us as f64 / 1_000_000.0);
            } else {
                // Last resort: estimate from file size
                let file_size = unsafe {
                    let pb = (*self.input.as_ptr()).pb;
                    if !pb.is_null() {
                        ffmpeg::ffi::avio_size(pb)
                    } else {
                        0
                    }
                };
                if file_size > 0 {
                    // Rough estimate: assume ~6 Mbps for MPEG-2
                    let estimated_bitrate = 6_000_000i64;
                    self.duration_us = (file_size * 8 * 1_000_000) / estimated_bitrate;
                    log::info!("Duration estimated from file size: {:.2}s",
                        self.duration_us as f64 / 1_000_000.0);
                } else {
                    self.duration_us = 10 * 60 * 1_000_000;
                    log::warn!("Using default duration of 10 minutes for unknown elementary stream");
                }
            }

            // Seek back to beginning for playback
            log::debug!("Seeking back to beginning after duration scan...");

            // For elementary streams, we need to reopen or use avformat_seek_file
            // Try byte-based seek first
            let seek_back_result = unsafe {
                ffmpeg::ffi::avformat_seek_file(
                    self.input.as_mut_ptr(),
                    -1,
                    i64::MIN,
                    0,
                    0,
                    ffmpeg::ffi::AVSEEK_FLAG_BYTE as i32,
                )
            };

            if seek_back_result < 0 {
                log::warn!("avformat_seek_file to beginning failed ({}), trying av_seek_frame", seek_back_result);
                let _ = unsafe {
                    ffmpeg::ffi::av_seek_frame(
                        self.input.as_mut_ptr(),
                        -1,
                        0,
                        ffmpeg::ffi::AVSEEK_FLAG_BYTE as i32,
                    )
                };
            }

            video_decoder.flush();
            log::debug!("Seek back completed, decoder flushed");
        } else if self.duration_us < 0 {
            // Negative duration is an error
            log::warn!("Duration calculation resulted in negative value {}, setting to 0", self.duration_us);
            self.duration_us = 0;
        }

        log::info!(
            "Video: {}x{} @ {:.2} fps, duration: {:.2}s",
            self.width,
            self.height,
            self.frame_rate,
            self.duration_us as f64 / 1_000_000.0
        );

        // Initialize scaler for pixel format conversion
        let target_ffmpeg_format = Self::pixel_format_to_ffmpeg(self.target_format);
        let source_format = video_decoder.format();

        if source_format != target_ffmpeg_format {
            let scaler = ScalerContext::get(
                source_format,
                self.width,
                self.height,
                target_ffmpeg_format,
                self.width,
                self.height,
                ScalerFlags::BILINEAR,
            )
            .map_err(|e| Error::DecodeFailed(format!("Failed to create scaler: {}", e)))?;

            self.scaler = Some(scaler);
            log::debug!(
                "Scaler initialized: {:?} -> {:?}",
                source_format,
                target_ffmpeg_format
            );
        }

        self.video_decoder = Some(video_decoder);
        Ok(())
    }

    /// Initialize audio decoder for a stream
    fn init_audio_decoder(&mut self, stream_index: usize) -> Result<()> {
        let stream = self.input.stream(stream_index).ok_or_else(|| {
            Error::InvalidFormat(format!("Audio stream {} not found", stream_index))
        })?;

        // Get codec parameters
        let codec_params = stream.parameters();
        self.audio_time_base = stream.time_base();

        // Find decoder
        let decoder_codec = ffmpeg::decoder::find(codec_params.id()).ok_or_else(|| {
            Error::CodecNotSupported(format!("No decoder for audio codec: {:?}", codec_params.id()))
        })?;

        log::info!(
            "Using audio decoder: {} ({})",
            decoder_codec.name(),
            decoder_codec.description()
        );

        // Create decoder context
        let mut decoder_ctx = CodecContext::new_with_codec(decoder_codec);
        decoder_ctx.set_parameters(codec_params).map_err(|e| {
            Error::DecodeFailed(format!("Failed to set audio codec parameters: {}", e))
        })?;

        // Open decoder
        let audio_decoder = decoder_ctx.decoder().audio().map_err(|e| {
            Error::DecodeFailed(format!("Failed to open audio decoder: {}", e))
        })?;

        self.audio_sample_rate = audio_decoder.rate();
        self.audio_channels = audio_decoder.channels() as u32;

        log::info!(
            "Audio: {} Hz, {} channels, format: {:?}",
            self.audio_sample_rate,
            self.audio_channels,
            audio_decoder.format()
        );

        // Create resampler to convert to float32 stereo at target sample rate
        let source_format = audio_decoder.format();
        let source_rate = audio_decoder.rate();
        let source_channels = audio_decoder.channels() as u32;

        // Get channel layout - if empty, create one from channel count
        let source_layout = {
            let layout = audio_decoder.channel_layout();
            if layout.is_empty() {
                // Create layout from channel count
                match source_channels {
                    1 => ffmpeg::channel_layout::ChannelLayout::MONO,
                    2 => ffmpeg::channel_layout::ChannelLayout::STEREO,
                    _ => {
                        log::warn!("Unsupported channel count: {}, defaulting to stereo", source_channels);
                        ffmpeg::channel_layout::ChannelLayout::STEREO
                    }
                }
            } else {
                layout
            }
        };

        // Target: stereo, float32, 48kHz
        let target_layout = ffmpeg::channel_layout::ChannelLayout::STEREO;
        let target_format = ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed);

        // Create resampler
        let resampler = ResamplerContext::get(
            source_format,
            source_layout,
            source_rate,
            target_format,
            target_layout,
            self.target_sample_rate,
        )
        .map_err(|e| Error::DecodeFailed(format!("Failed to create audio resampler: {}", e)))?;

        log::info!(
            "Audio resampler: {:?} {:?} {}Hz -> {:?} {:?} {}Hz",
            source_format,
            source_layout,
            source_rate,
            target_format,
            target_layout,
            self.target_sample_rate
        );

        self.resampler = Some(resampler);
        self.audio_decoder = Some(audio_decoder);
        Ok(())
    }

    /// Get media information
    pub fn get_media_info(&self) -> Result<MediaInfo> {
        let mut video_tracks = Vec::new();
        let mut audio_tracks = Vec::new();
        let mut metadata = HashMap::new();

        // Extract metadata
        for (key, value) in self.input.metadata().iter() {
            metadata.insert(key.to_string(), value.to_string());
        }

        // Process video streams
        for stream in self.input.streams() {
            let params = stream.parameters();
            let medium = params.medium();

            if medium == MediaType::Video {
                let codec_id = params.id();
                let codec = ffmpeg::decoder::find(codec_id);

                let codec_info = CodecInfo {
                    name: codec.map(|c| c.name().to_string()).unwrap_or_default(),
                    long_name: codec
                        .map(|c| c.description().to_string())
                        .unwrap_or_default(),
                    four_cc: Self::get_fourcc(codec_id),
                };

                let frame_rate = stream.avg_frame_rate();
                let fps = if frame_rate.denominator() > 0 {
                    frame_rate.numerator() as f64 / frame_rate.denominator() as f64
                } else {
                    0.0
                };

                let video_track = VideoTrack {
                    index: stream.index() as i32,
                    codec: codec_info,
                    width: unsafe { (*params.as_ptr()).width },
                    height: unsafe { (*params.as_ptr()).height },
                    frame_rate: fps,
                    bit_rate: unsafe { (*params.as_ptr()).bit_rate },
                    pixel_format: Self::get_pixel_format_name(params),
                    is_hardware_decodable: Self::is_hardware_decodable(codec_id),
                    color_space: None,
                    color_primaries: None,
                    color_transfer: None,
                    color_range: "unknown".to_string(),
                };

                video_tracks.push(video_track);
            } else if medium == MediaType::Audio {
                let codec_id = params.id();
                let codec = ffmpeg::decoder::find(codec_id);

                let codec_info = CodecInfo {
                    name: codec.map(|c| c.name().to_string()).unwrap_or_default(),
                    long_name: codec
                        .map(|c| c.description().to_string())
                        .unwrap_or_default(),
                    four_cc: None,
                };

                let audio_track = AudioTrack {
                    index: stream.index() as i32,
                    codec: codec_info,
                    sample_rate: unsafe { (*params.as_ptr()).sample_rate },
                    channels: unsafe { (*params.as_ptr()).ch_layout.nb_channels },
                    channel_layout: None,
                    bit_rate: unsafe { (*params.as_ptr()).bit_rate },
                    language_code: stream
                        .metadata()
                        .get("language")
                        .map(|s| s.to_string()),
                };

                audio_tracks.push(audio_track);
            }
        }

        // Get container format
        let container_format = self
            .input
            .format()
            .name()
            .split(',')
            .next()
            .unwrap_or("unknown")
            .to_string();

        // Calculate duration
        let duration = if self.duration_us > 0 {
            self.duration_us as f64 / 1_000_000.0
        } else {
            let container_duration = self.input.duration();
            container_duration as f64 / ffmpeg::ffi::AV_TIME_BASE as f64
        };

        Ok(MediaInfo {
            duration,
            container_format,
            video_tracks,
            audio_tracks,
            metadata,
        })
    }

    /// Seek to a specific time in microseconds (seeks to nearest keyframe)
    pub fn seek(&mut self, time_us: i64) -> Result<()> {
        log::info!(
            "FFmpegContext::seek - time_us={}, time_base={}/{}",
            time_us,
            self.video_time_base.numerator(),
            self.video_time_base.denominator()
        );

        // The seek function uses stream index -1 which means AV_TIME_BASE (microseconds)
        // Per ffmpeg-next docs: seek(timestamp, range) where range is ..timestamp
        // to seek to a keyframe at or before the target position
        log::info!("FFmpegContext::seek - calling input.seek() with target={} us", time_us);

        // Try timestamp-based seek first
        let seek_result = self.input.seek(time_us, ..time_us);

        if let Err(e) = seek_result {
            log::warn!("FFmpegContext::seek - timestamp seek failed: {}, trying byte-based seek", e);

            // For elementary streams (like .m2v), timestamp seek may fail
            // Fall back to byte-based seek
            if time_us == 0 {
                // Seek to beginning - use byte position 0
                let byte_seek_result = unsafe {
                    ffmpeg::ffi::av_seek_frame(
                        self.input.as_mut_ptr(),
                        -1,
                        0,
                        ffmpeg::ffi::AVSEEK_FLAG_BYTE as i32,
                    )
                };

                if byte_seek_result < 0 {
                    // Try avformat_seek_file as last resort
                    let file_seek_result = unsafe {
                        ffmpeg::ffi::avformat_seek_file(
                            self.input.as_mut_ptr(),
                            -1,
                            i64::MIN,
                            0,
                            0,
                            ffmpeg::ffi::AVSEEK_FLAG_BYTE as i32,
                        )
                    };
                    if file_seek_result < 0 {
                        log::error!("FFmpegContext::seek - all seek methods failed");
                        return Err(Error::SeekFailed(time_us));
                    }
                }
                log::info!("FFmpegContext::seek - byte seek to beginning succeeded");
            } else {
                // For non-zero seeks, try to estimate byte position
                // This is a rough estimate based on file position
                if self.duration_us > 0 {
                    let file_size = unsafe {
                        let pb = (*self.input.as_ptr()).pb;
                        if !pb.is_null() {
                            ffmpeg::ffi::avio_size(pb)
                        } else {
                            0
                        }
                    };

                    if file_size > 0 {
                        // Estimate byte position proportionally
                        let byte_pos = (file_size as f64 * (time_us as f64 / self.duration_us as f64)) as i64;
                        let byte_seek_result = unsafe {
                            ffmpeg::ffi::av_seek_frame(
                                self.input.as_mut_ptr(),
                                -1,
                                byte_pos,
                                ffmpeg::ffi::AVSEEK_FLAG_BYTE as i32,
                            )
                        };
                        if byte_seek_result < 0 {
                            log::error!("FFmpegContext::seek - byte seek to {} failed", byte_pos);
                            return Err(Error::SeekFailed(time_us));
                        }
                        log::info!("FFmpegContext::seek - byte seek to position {} succeeded", byte_pos);
                    } else {
                        return Err(Error::SeekFailed(time_us));
                    }
                } else {
                    return Err(Error::SeekFailed(time_us));
                }
            }
        } else {
            log::info!("FFmpegContext::seek - timestamp seek succeeded");
        }

        // Flush decoder buffers - critical after seek!
        if let Some(ref mut decoder) = self.video_decoder {
            log::info!("FFmpegContext::seek - flushing video decoder");
            decoder.flush();
        }

        // Flush audio decoder and clear packet queues
        if let Some(ref mut decoder) = self.audio_decoder {
            log::info!("FFmpegContext::seek - flushing audio decoder");
            decoder.flush();
        }

        // Flush the resampler to clear any buffered samples from before the seek.
        // This is critical for MPEG audio (MP2/MP3) which uses overlapping synthesis windows.
        if let Some(ref mut resampler) = self.resampler {
            log::info!("FFmpegContext::seek - flushing audio resampler");
            // Create a temporary output frame to receive any remaining samples (discard them)
            let target_format = ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed);
            let target_layout = ffmpeg::channel_layout::ChannelLayout::STEREO;
            let mut flush_output = ffmpeg::frame::Audio::new(target_format, 4096, target_layout);
            // Flush may fail if no samples buffered, ignore the error
            let _ = resampler.flush(&mut flush_output);
        }

        self.audio_packet_queue.clear();
        self.video_packet_queue.clear();

        // Reset frame counters for accurate tracking after seek
        self.frame_number = 0;
        self.audio_frame_number = 0;

        log::info!("FFmpegContext::seek - complete");
        Ok(())
    }

    /// Seek precisely to a specific time in microseconds.
    /// This performs a keyframe seek first, then decodes frames until reaching the target time.
    /// Returns the frame at or just before the target time (frame-accurate seek).
    pub fn seek_precise(&mut self, time_us: i64) -> Result<Option<VideoFrame>> {
        log::info!(
            "FFmpegContext::seek_precise - time_us={}, time_base={}/{}",
            time_us,
            self.video_time_base.numerator(),
            self.video_time_base.denominator()
        );

        // First, seek to keyframe at or before target
        self.seek(time_us)?;

        // Now decode frames until we reach the target time
        // We need to find the frame at or just before time_us
        let mut best_frame: Option<VideoFrame> = None;
        let max_frames = 300; // Limit to prevent infinite loop (enough for ~10 seconds at 30fps)
        let mut frame_count = 0;

        log::info!("FFmpegContext::seek_precise - decoding frames to reach target");

        loop {
            if frame_count >= max_frames {
                log::warn!(
                    "FFmpegContext::seek_precise - exceeded max frame count ({}), returning best frame",
                    max_frames
                );
                break;
            }

            match self.decode_next_frame()? {
                Some(frame) => {
                    frame_count += 1;
                    let frame_pts = frame.pts_us;

                    log::debug!(
                        "FFmpegContext::seek_precise - decoded frame {}: pts={} us (target: {} us)",
                        frame_count,
                        frame_pts,
                        time_us
                    );

                    // Check if this frame is at or before the target
                    if frame_pts <= time_us {
                        // This frame is a candidate - keep it as best
                        best_frame = Some(frame);

                        // Check if we're close enough (within one frame duration)
                        let frame_duration_us = if self.frame_rate > 0.0 {
                            (1_000_000.0 / self.frame_rate) as i64
                        } else {
                            33333 // ~30fps default
                        };

                        // If the next frame would be past the target, we found our frame
                        if frame_pts + frame_duration_us > time_us {
                            log::info!(
                                "FFmpegContext::seek_precise - found target frame at {} us (target: {} us, delta: {} us)",
                                frame_pts,
                                time_us,
                                time_us - frame_pts
                            );
                            break;
                        }
                    } else {
                        // Frame is past the target
                        if best_frame.is_some() {
                            // We already have a frame before target, use it
                            log::info!(
                                "FFmpegContext::seek_precise - frame {} us is past target {} us, using previous frame",
                                frame_pts,
                                time_us
                            );
                        } else {
                            // No frame before target found (target is before first keyframe after seek)
                            // Return this frame as best effort
                            log::info!(
                                "FFmpegContext::seek_precise - no frame before target, using first available at {} us",
                                frame_pts
                            );
                            best_frame = Some(frame);
                        }
                        break;
                    }
                }
                None => {
                    // End of stream
                    log::info!("FFmpegContext::seek_precise - end of stream reached");
                    break;
                }
            }
        }

        if let Some(ref frame) = best_frame {
            log::info!(
                "FFmpegContext::seek_precise - complete: returning frame at {} us (target: {} us, decoded {} frames)",
                frame.pts_us,
                time_us,
                frame_count
            );
        } else {
            log::warn!(
                "FFmpegContext::seek_precise - no frame found (decoded {} frames)",
                frame_count
            );
        }

        // Clear audio packet queue after seek_precise.
        // During seek_precise, audio packets are queued while decoding video frames to reach target.
        // These audio packets may not be synchronized with the final video position.
        // Clearing the queue ensures that prime_audio_after_seek will read fresh audio packets
        // starting from the correct position.
        if !self.audio_packet_queue.is_empty() {
            log::info!(
                "FFmpegContext::seek_precise - clearing {} stale audio packets from queue",
                self.audio_packet_queue.len()
            );
            self.audio_packet_queue.clear();
        }

        Ok(best_frame)
    }

    /// Prime the audio decoder after seek by pre-reading packets into queues.
    /// This ensures that audio packets are available for decode_next_audio_frame() after seek.
    /// The function reads from the input stream and queues both audio and video packets
    /// so that subsequent calls to decode_next_audio_frame() or decode_next_frame()
    /// can process them without hitting empty queues due to interleaved packet ordering.
    /// Returns the number of audio packets that were queued.
    pub fn prime_audio_after_seek(&mut self) -> Result<u32> {
        if self.audio_decoder.is_none() {
            log::debug!("prime_audio_after_seek - no audio decoder");
            return Ok(0);
        }

        let audio_stream_idx = match self.audio_stream_index {
            Some(idx) => idx,
            None => {
                log::debug!("prime_audio_after_seek - no audio stream");
                return Ok(0);
            }
        };

        // Flush audio decoder to clear any stale decoded frames from previous position.
        // This is critical after seek to ensure clean audio from the new position.
        if let Some(ref mut decoder) = self.audio_decoder {
            log::info!("prime_audio_after_seek - flushing audio decoder");
            decoder.flush();
        }

        // Flush resampler to clear any buffered samples
        if let Some(ref mut resampler) = self.resampler {
            log::info!("prime_audio_after_seek - flushing audio resampler");
            let target_format = ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed);
            let target_layout = ffmpeg::channel_layout::ChannelLayout::STEREO;
            let mut flush_output = ffmpeg::frame::Audio::new(target_format, 4096, target_layout);
            let _ = resampler.flush(&mut flush_output);
        }

        log::info!("prime_audio_after_seek - starting, audio_queue={}, video_queue={}",
            self.audio_packet_queue.len(), self.video_packet_queue.len());

        let max_packets = 200; // Limit packet reads to avoid reading too far
        let target_audio_packets = 10; // Target number of audio packets to queue
        let mut packet_count = 0;
        let mut audio_packets_queued = 0;
        let mut video_packets_queued = 0;

        // Read packets until we have enough audio packets queued
        while packet_count < max_packets && audio_packets_queued < target_audio_packets {
            match self.input.packets().next() {
                Some((stream, packet)) => {
                    packet_count += 1;
                    if stream.index() == audio_stream_idx {
                        // Queue audio packet for later decoding
                        log::trace!("prime_audio_after_seek - queueing audio packet {}", audio_packets_queued + 1);
                        self.audio_packet_queue.push_back(packet);
                        audio_packets_queued += 1;
                    } else if Some(stream.index()) == self.video_stream_index {
                        // Queue video packets for later video decoding
                        log::trace!("prime_audio_after_seek - queueing video packet");
                        self.video_packet_queue.push_back(packet);
                        video_packets_queued += 1;
                    }
                    // Skip other streams (subtitles, etc.)
                }
                None => {
                    log::info!("prime_audio_after_seek - end of stream reached");
                    break;
                }
            }
        }

        log::info!("prime_audio_after_seek - done: read {} packets, queued {} audio + {} video, audio_queue={}, video_queue={}",
            packet_count, audio_packets_queued, video_packets_queued,
            self.audio_packet_queue.len(), self.video_packet_queue.len());

        Ok(audio_packets_queued)
    }

    /// Decode the next frame
    pub fn decode_next_frame(&mut self) -> Result<Option<VideoFrame>> {
        log::debug!("decode_next_frame - start");

        let video_stream_idx = match self.video_stream_index {
            Some(idx) => idx,
            None => {
                log::warn!("decode_next_frame - no video stream index");
                return Ok(None);
            }
        };

        if self.video_decoder.is_none() {
            log::warn!("decode_next_frame - no video decoder");
            return Ok(None);
        }

        let mut packet_count = 0;
        let max_packets = 500; // Limit to prevent infinite loop

        loop {
            if packet_count >= max_packets {
                log::warn!("decode_next_frame - exceeded max packet count ({})", max_packets);
                return Ok(None);
            }

            log::trace!("decode_next_frame - reading packet {}, video_queue={}", packet_count, self.video_packet_queue.len());

            // First, try to get video packets from the queue (collected during audio decoding)
            let packet = if let Some(queued_packet) = self.video_packet_queue.pop_front() {
                log::trace!("decode_next_frame - using queued video packet, remaining={}", self.video_packet_queue.len());
                packet_count += 1;
                Some(queued_packet)
            } else {
                // Queue is empty, read from stream
                match self.input.packets().next() {
                    Some((stream, packet)) => {
                        packet_count += 1;
                        if stream.index() == video_stream_idx {
                            log::trace!("decode_next_frame - got video packet {}", packet_count);
                            Some(packet)
                        } else if Some(stream.index()) == self.audio_stream_index {
                            // Queue audio packets for later decoding
                            log::trace!("decode_next_frame - queueing audio packet (stream {})", stream.index());
                            self.audio_packet_queue.push_back(packet);
                            continue;
                        } else {
                            log::trace!("decode_next_frame - skipping other packet (stream {})", stream.index());
                            continue; // Skip other streams (subtitles, etc.)
                        }
                    }
                    None => {
                        log::info!("decode_next_frame - end of stream, flushing decoder");
                        // End of stream - flush decoder
                        if let Some(ref mut decoder) = self.video_decoder {
                            decoder.send_eof().ok();
                        }
                        return self.receive_frame();
                    }
                }
            };

            if let Some(packet) = packet {
                // Send packet to decoder
                if let Some(ref mut decoder) = self.video_decoder {
                    log::trace!("decode_next_frame - sending packet to decoder");
                    decoder.send_packet(&packet).map_err(|e| {
                        log::error!("decode_next_frame - send_packet failed: {}", e);
                        Error::DecodeFailed(format!("Failed to send packet: {}", e))
                    })?;
                }

                // Try to receive a frame
                log::trace!("decode_next_frame - trying to receive frame");
                if let Some(frame) = self.receive_frame()? {
                    log::info!("decode_next_frame - got frame: pts={} us, {}x{}",
                        frame.pts_us, frame.width, frame.height);
                    return Ok(Some(frame));
                }
            }
        }
    }

    /// Receive a decoded frame from the decoder
    fn receive_frame(&mut self) -> Result<Option<VideoFrame>> {
        let decoder = match self.video_decoder.as_mut() {
            Some(d) => d,
            None => return Ok(None),
        };

        let mut decoded = VideoFrameFFmpeg::empty();

        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                // Get timestamp BEFORE scaling (scaling may lose timestamp info)
                // Try pts first, then best_effort_timestamp for formats like WMV
                let pts = decoded.pts()
                    .or_else(|| {
                        // Access best_effort_timestamp via unsafe FFI
                        let timestamp = unsafe { (*decoded.as_ptr()).best_effort_timestamp };
                        if timestamp != ffmpeg::ffi::AV_NOPTS_VALUE {
                            Some(timestamp)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);

                let is_keyframe = decoded.is_key();

                log::debug!("receive_frame - raw pts={}, is_keyframe={}", pts, is_keyframe);

                // Convert frame to target format
                let output_frame = if let Some(ref mut scaler) = self.scaler {
                    let mut scaled = VideoFrameFFmpeg::empty();
                    scaler.run(&decoded, &mut scaled).map_err(|e| {
                        Error::DecodeFailed(format!("Failed to scale frame: {}", e))
                    })?;
                    scaled
                } else {
                    decoded
                };

                // Extract frame data using the pre-scaling timestamp
                let frame = self.create_video_frame_with_pts(&output_frame, pts, is_keyframe)?;
                self.frame_number += 1;

                Ok(Some(frame))
            }
            Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                // Need more data
                Ok(None)
            }
            Err(ffmpeg::Error::Eof) => {
                // End of stream
                Ok(None)
            }
            Err(e) => Err(Error::DecodeFailed(format!("Failed to receive frame: {}", e))),
        }
    }

    /// Create a VideoFrame from FFmpeg frame (legacy, uses frame's PTS)
    #[allow(dead_code)]
    fn create_video_frame(&self, frame: &VideoFrameFFmpeg) -> Result<VideoFrame> {
        let pts = frame.pts().unwrap_or(0);
        let is_keyframe = frame.is_key();
        self.create_video_frame_with_pts(frame, pts, is_keyframe)
    }

    /// Create a VideoFrame from FFmpeg frame with explicit PTS and keyframe info
    /// This is needed because scaling may lose timestamp/keyframe information
    fn create_video_frame_with_pts(&self, frame: &VideoFrameFFmpeg, pts: i64, is_keyframe: bool) -> Result<VideoFrame> {
        let width = frame.width();
        let height = frame.height();
        let stride = frame.stride(0) as u32;

        // Calculate PTS in microseconds
        let pts_us = Self::pts_to_us(pts, self.video_time_base);

        // Calculate frame duration
        let frame_duration_us = if self.frame_rate > 0.0 {
            (1_000_000.0 / self.frame_rate) as i64
        } else {
            16666 // Default to ~60fps
        };

        // Copy pixel data
        let data = match self.target_format {
            PixelFormat::Bgra => {
                let plane = frame.data(0);
                let size = (stride * height) as usize;
                plane[..size].to_vec()
            }
            PixelFormat::Nv12 => {
                // NV12: Y plane + interleaved UV plane
                let y_plane = frame.data(0);
                let uv_plane = frame.data(1);
                let y_size = (stride * height) as usize;
                let uv_size = (stride * height / 2) as usize;

                let mut data = Vec::with_capacity(y_size + uv_size);
                data.extend_from_slice(&y_plane[..y_size]);
                data.extend_from_slice(&uv_plane[..uv_size]);
                data
            }
            PixelFormat::Yuv420p => {
                // YUV420P: Y plane + U plane + V plane
                let y_plane = frame.data(0);
                let u_plane = frame.data(1);
                let v_plane = frame.data(2);
                let y_size = (stride * height) as usize;
                let uv_size = (stride * height / 4) as usize;

                let mut data = Vec::with_capacity(y_size + uv_size * 2);
                data.extend_from_slice(&y_plane[..y_size]);
                data.extend_from_slice(&u_plane[..uv_size]);
                data.extend_from_slice(&v_plane[..uv_size]);
                data
            }
        };

        Ok(VideoFrame::new(
            data,
            width,
            height,
            stride,
            pts_us,
            frame_duration_us,
            is_keyframe,
            self.frame_number,
            self.target_format,
        ))
    }

    /// Convert PTS to microseconds
    fn pts_to_us(pts: i64, time_base: Rational) -> i64 {
        if time_base.denominator() == 0 {
            return pts;
        }
        (pts * 1_000_000 * time_base.numerator() as i64) / time_base.denominator() as i64
    }

    /// Convert microseconds to PTS
    fn us_to_pts(us: i64, time_base: Rational) -> i64 {
        if time_base.numerator() == 0 {
            return us;
        }
        (us * time_base.denominator() as i64) / (1_000_000 * time_base.numerator() as i64)
    }

    /// Convert our PixelFormat to FFmpeg format
    fn pixel_format_to_ffmpeg(format: PixelFormat) -> ffmpeg::format::Pixel {
        match format {
            PixelFormat::Bgra => ffmpeg::format::Pixel::BGRA,
            PixelFormat::Nv12 => ffmpeg::format::Pixel::NV12,
            PixelFormat::Yuv420p => ffmpeg::format::Pixel::YUV420P,
        }
    }

    /// Get FourCC for a codec
    fn get_fourcc(codec_id: ffmpeg::codec::Id) -> Option<String> {
        match codec_id {
            ffmpeg::codec::Id::H264 => Some("avc1".to_string()),
            ffmpeg::codec::Id::HEVC => Some("hvc1".to_string()),
            ffmpeg::codec::Id::VP9 => Some("vp09".to_string()),
            ffmpeg::codec::Id::AV1 => Some("av01".to_string()),
            ffmpeg::codec::Id::PRORES => Some("apch".to_string()),
            ffmpeg::codec::Id::MPEG2VIDEO => Some("m2v1".to_string()),
            ffmpeg::codec::Id::MPEG4 => Some("mp4v".to_string()),
            ffmpeg::codec::Id::DNXHD => Some("AVdn".to_string()),
            _ => None,
        }
    }

    /// Check if codec is VideoToolbox decodable
    fn is_hardware_decodable(codec_id: ffmpeg::codec::Id) -> bool {
        matches!(
            codec_id,
            ffmpeg::codec::Id::H264
                | ffmpeg::codec::Id::HEVC
                | ffmpeg::codec::Id::VP9
                | ffmpeg::codec::Id::PRORES
        )
    }

    /// Get pixel format name from codec parameters
    fn get_pixel_format_name(params: ffmpeg::codec::Parameters) -> String {
        let format = unsafe { (*params.as_ptr()).format };

        // Map common pixel format codes to names
        match format {
            0 => "yuv420p".to_string(),    // AV_PIX_FMT_YUV420P
            3 => "rgb24".to_string(),      // AV_PIX_FMT_RGB24
            23 => "nv12".to_string(),      // AV_PIX_FMT_NV12
            28 => "bgra".to_string(),      // AV_PIX_FMT_BGRA
            4 => "yuv422p".to_string(),    // AV_PIX_FMT_YUV422P
            5 => "yuv444p".to_string(),    // AV_PIX_FMT_YUV444P
            _ => format!("pix_fmt_{}", format),
        }
    }

    /// Get video stream index
    pub fn video_stream_index(&self) -> Option<usize> {
        self.video_stream_index
    }

    /// Get audio stream index
    pub fn audio_stream_index(&self) -> Option<usize> {
        self.audio_stream_index
    }

    /// Get frame rate
    pub fn frame_rate(&self) -> f64 {
        self.frame_rate
    }

    /// Get video dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get duration in microseconds
    pub fn duration_us(&self) -> i64 {
        self.duration_us
    }

    /// Get audio sample rate (returns target/output sample rate after resampling)
    pub fn audio_sample_rate(&self) -> u32 {
        // Return target sample rate since we resample to this rate
        if self.resampler.is_some() {
            self.target_sample_rate
        } else {
            self.audio_sample_rate
        }
    }

    /// Get audio channels (returns target/output channel count after resampling)
    pub fn audio_channels(&self) -> u32 {
        // Return target channels since we resample to stereo
        if self.resampler.is_some() {
            self.target_channels
        } else {
            self.audio_channels
        }
    }

    /// Check if audio is available
    pub fn has_audio(&self) -> bool {
        self.audio_decoder.is_some()
    }

    /// Decode the next audio frame
    pub fn decode_next_audio_frame(&mut self) -> Result<Option<AudioFrame>> {
        log::debug!("decode_next_audio_frame - start, queue_size={}", self.audio_packet_queue.len());

        let audio_stream_idx = match self.audio_stream_index {
            Some(idx) => {
                log::debug!("decode_next_audio_frame - audio stream index={}", idx);
                idx
            }
            None => {
                log::warn!("decode_next_audio_frame - no audio stream index");
                return Ok(None);
            }
        };

        if self.audio_decoder.is_none() {
            log::warn!("decode_next_audio_frame - no audio decoder");
            return Ok(None);
        }

        let max_packets = 500;
        let mut packet_count = 0;

        loop {
            if packet_count >= max_packets {
                log::warn!("decode_next_audio_frame - exceeded max packet count");
                return Ok(None);
            }

            // First, try to get audio packets from the queue (collected during video decoding)
            let packet = if let Some(queued_packet) = self.audio_packet_queue.pop_front() {
                log::trace!("decode_next_audio_frame - using queued packet, remaining={}", self.audio_packet_queue.len());
                packet_count += 1;
                Some(queued_packet)
            } else {
                // Queue is empty, read from stream
                match self.input.packets().next() {
                    Some((stream, packet)) => {
                        packet_count += 1;
                        if stream.index() == audio_stream_idx {
                            Some(packet)
                        } else if Some(stream.index()) == self.video_stream_index {
                            // Queue video packets for later decoding
                            log::trace!("decode_next_audio_frame - queueing video packet (stream {})", stream.index());
                            self.video_packet_queue.push_back(packet);
                            continue;
                        } else {
                            // Skip other streams (subtitles, etc.)
                            continue;
                        }
                    }
                    None => {
                        // End of stream - flush decoder
                        if let Some(ref mut decoder) = self.audio_decoder {
                            decoder.send_eof().ok();
                        }
                        return self.receive_audio_frame();
                    }
                }
            };

            if let Some(packet) = packet {
                log::debug!("decode_next_audio_frame - sending audio packet {} to decoder", packet_count);
                // Send packet to decoder
                if let Some(ref mut decoder) = self.audio_decoder {
                    match decoder.send_packet(&packet) {
                        Ok(()) => {
                            // Packet sent successfully
                        }
                        Err(e) => {
                            // After seek, MP2/MP3 decoder may fail on first few packets
                            // because they don't start at a valid frame boundary (sync header).
                            // Skip invalid packets and continue to the next one.
                            log::warn!(
                                "decode_next_audio_frame - send_packet failed (skipping): {}",
                                e
                            );
                            continue;
                        }
                    }
                }

                // Try to receive a frame
                if let Some(frame) = self.receive_audio_frame()? {
                    log::debug!("decode_next_audio_frame - returning frame with {} samples", frame.sample_count);
                    return Ok(Some(frame));
                }
            }
        }
    }

    /// Receive a decoded audio frame from the decoder
    fn receive_audio_frame(&mut self) -> Result<Option<AudioFrame>> {
        let decoder = match self.audio_decoder.as_mut() {
            Some(d) => d,
            None => return Ok(None),
        };

        let mut decoded = AudioFrameFFmpeg::empty();

        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                // Get timestamp
                let pts = decoded.pts().unwrap_or(0);
                let pts_us = Self::pts_to_us(pts, self.audio_time_base);

                log::debug!(
                    "receive_audio_frame - decoded: pts={}, samples={}, rate={}, channels={}, format={:?}",
                    pts,
                    decoded.samples(),
                    decoded.rate(),
                    decoded.channels(),
                    decoded.format()
                );

                // Convert to float32 stereo using resampler
                let frame = self.convert_audio_frame(&decoded, pts_us)?;
                self.audio_frame_number += 1;

                if frame.sample_count > 0 {
                    log::debug!(
                        "receive_audio_frame - output: samples={}, channels={}, data_len={}",
                        frame.sample_count,
                        frame.channels,
                        frame.data.len()
                    );
                }

                Ok(Some(frame))
            }
            Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                // Need more data
                log::trace!("receive_audio_frame - EAGAIN, need more data");
                Ok(None)
            }
            Err(ffmpeg::Error::Eof) => {
                // End of stream
                log::debug!("receive_audio_frame - EOF");
                Ok(None)
            }
            Err(e) => Err(Error::DecodeFailed(format!("Failed to receive audio frame: {}", e))),
        }
    }

    /// Convert FFmpeg audio frame to our AudioFrame format
    fn convert_audio_frame(&mut self, frame: &AudioFrameFFmpeg, pts_us: i64) -> Result<AudioFrame> {
        let resampler = match self.resampler.as_mut() {
            Some(r) => r,
            None => {
                return Err(Error::DecodeFailed("No audio resampler available".to_string()));
            }
        };

        // Pre-allocate output frame with target format
        // Calculate expected output samples based on sample rate conversion
        let input_samples = frame.samples();
        let input_rate = frame.rate() as u64;
        let output_rate = self.target_sample_rate as u64;

        // Calculate output samples (with some extra buffer for rounding)
        let expected_output_samples = if input_rate > 0 {
            ((input_samples as u64 * output_rate + input_rate - 1) / input_rate) as usize + 32
        } else {
            input_samples + 32
        };

        let target_format = ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed);
        let target_layout = ffmpeg::channel_layout::ChannelLayout::STEREO;

        // Create and allocate output frame
        let mut resampled = ffmpeg::frame::Audio::new(target_format, expected_output_samples, target_layout);

        // Set the sample rate on output frame
        unsafe {
            (*resampled.as_mut_ptr()).sample_rate = self.target_sample_rate as i32;
        }

        // Fix input frame channel layout if empty (some decoders don't set it)
        // The resampler needs matching channel layout to what it was initialized with
        let frame_layout = frame.channel_layout();
        let input_frame = if frame_layout.is_empty() {
            // Clone the frame and set the channel layout
            let mut fixed_frame = unsafe {
                let mut new_frame = ffmpeg::frame::Audio::empty();
                // Copy frame data via FFI
                ffmpeg::ffi::av_frame_ref(new_frame.as_mut_ptr(), frame.as_ptr());
                new_frame
            };

            // Set channel layout based on channel count
            let layout = match frame.channels() {
                1 => ffmpeg::channel_layout::ChannelLayout::MONO,
                2 => ffmpeg::channel_layout::ChannelLayout::STEREO,
                _ => ffmpeg::channel_layout::ChannelLayout::STEREO,
            };
            fixed_frame.set_channel_layout(layout);

            log::debug!(
                "convert_audio_frame - fixed empty channel layout to {:?}",
                layout
            );

            Some(fixed_frame)
        } else {
            None
        };

        // Use the fixed frame if we created one, otherwise use original
        let input_ref = input_frame.as_ref().unwrap_or(frame);

        log::trace!(
            "convert_audio_frame - input: samples={}, rate={}, channels={}, format={:?}, layout={:?}",
            input_ref.samples(),
            input_ref.rate(),
            input_ref.channels(),
            input_ref.format(),
            input_ref.channel_layout()
        );

        // Run resampler - use run() which handles the conversion
        let delay = resampler.run(input_ref, &mut resampled).map_err(|e| {
            log::error!("Audio resample failed: {}", e);
            Error::DecodeFailed(format!("Audio resample failed: {}", e))
        })?;

        // Get actual output sample count
        let actual_samples = resampled.samples();

        log::trace!(
            "convert_audio_frame - output: samples={}, delay={:?}",
            actual_samples,
            delay
        );

        if actual_samples == 0 {
            // Resampler may need more input data before producing output
            log::trace!("convert_audio_frame - no output samples yet (buffering)");

            // Return empty frame - caller should continue feeding input
            return Ok(AudioFrame::new(
                Vec::new(),
                0,
                self.target_channels,
                self.target_sample_rate,
                pts_us,
                0,
                self.audio_frame_number,
            ));
        }

        // Extract float32 samples from resampled frame
        let output_samples = Self::extract_float_samples_static(&resampled);

        let sample_count = if self.target_channels > 0 {
            output_samples.len() / self.target_channels as usize
        } else {
            0
        };

        log::trace!(
            "convert_audio_frame - extracted {} float samples ({} frames)",
            output_samples.len(),
            sample_count
        );

        let duration_us = AudioFrame::calculate_duration_us(
            sample_count as u32,
            self.target_sample_rate,
        );

        Ok(AudioFrame::new(
            output_samples,
            sample_count as u32,
            self.target_channels,
            self.target_sample_rate,
            pts_us,
            duration_us,
            self.audio_frame_number,
        ))
    }

    /// Extract float32 samples from an FFmpeg audio frame (static version)
    fn extract_float_samples_static(frame: &ffmpeg::frame::Audio) -> Vec<f32> {
        let samples = frame.samples();
        let channels = frame.channels() as usize;

        if samples == 0 || channels == 0 {
            return Vec::new();
        }

        let total_samples = samples * channels;
        let mut output = Vec::with_capacity(total_samples);

        // Get data from plane 0 (packed format)
        let data = frame.data(0);
        let float_slice = unsafe {
            std::slice::from_raw_parts(
                data.as_ptr() as *const f32,
                total_samples.min(data.len() / 4),
            )
        };

        output.extend_from_slice(float_slice);
        output
    }

    /// Seek audio stream
    pub fn seek_audio(&mut self, time_us: i64) -> Result<()> {
        log::debug!("seek_audio - seeking to {} us", time_us);

        // Seek using container-level seek (affects all streams)
        self.input
            .seek(time_us, ..time_us)
            .map_err(|e| Error::SeekFailed(time_us))?;

        // Flush audio decoder
        if let Some(ref mut decoder) = self.audio_decoder {
            decoder.flush();
        }

        self.audio_frame_number = 0;
        Ok(())
    }

    /// Flush audio decoder buffers
    pub fn flush_audio(&mut self) {
        if let Some(ref mut decoder) = self.audio_decoder {
            decoder.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pts_conversion() {
        let time_base = Rational::new(1, 90000);

        // 90000 pts @ 1/90000 = 1 second = 1000000 us
        let us = FFmpegContext::pts_to_us(90000, time_base);
        assert_eq!(us, 1_000_000);

        // Round trip
        let pts = FFmpegContext::us_to_pts(us, time_base);
        assert_eq!(pts, 90000);
    }

    #[test]
    fn test_pixel_format_conversion() {
        assert_eq!(
            FFmpegContext::pixel_format_to_ffmpeg(PixelFormat::Bgra),
            ffmpeg::format::Pixel::BGRA
        );
        assert_eq!(
            FFmpegContext::pixel_format_to_ffmpeg(PixelFormat::Nv12),
            ffmpeg::format::Pixel::NV12
        );
    }

    /// Test audio decoding with sample WMV file
    #[test]
    fn test_audio_decoding_wmv() {
        // Initialize logging for test
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .is_test(true)
            .try_init();

        let sample_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("samples/sample_960x400_ocean_with_audio.wmv");

        if !sample_path.exists() {
            eprintln!("Skipping test: sample file not found at {:?}", sample_path);
            return;
        }

        let config = DecoderConfig::default();
        let mut ctx = FFmpegContext::new(&sample_path, &config)
            .expect("Failed to create FFmpegContext");

        // Verify audio stream exists
        assert!(ctx.audio_stream_index.is_some(), "No audio stream found");
        assert!(ctx.audio_decoder.is_some(), "No audio decoder initialized");
        assert!(ctx.resampler.is_some(), "No resampler initialized");

        println!("Audio stream index: {:?}", ctx.audio_stream_index);
        println!("Source sample rate: {}", ctx.audio_sample_rate);
        println!("Source channels: {}", ctx.audio_channels);
        println!("Target sample rate: {}", ctx.target_sample_rate);
        println!("Target channels: {}", ctx.target_channels);

        // Try to decode audio frames
        let mut frame_count = 0;
        let mut total_samples = 0;
        let max_frames = 10;

        for _ in 0..max_frames {
            match ctx.decode_next_audio_frame() {
                Ok(Some(frame)) => {
                    println!(
                        "Audio frame {}: samples={}, channels={}, rate={}, data_len={}",
                        frame_count,
                        frame.sample_count,
                        frame.channels,
                        frame.sample_rate,
                        frame.data.len()
                    );

                    assert!(frame.sample_count > 0, "Frame should have samples");
                    assert_eq!(frame.channels, ctx.target_channels, "Channels mismatch");
                    assert_eq!(frame.sample_rate, ctx.target_sample_rate, "Sample rate mismatch");
                    assert_eq!(
                        frame.data.len(),
                        (frame.sample_count * frame.channels) as usize,
                        "Data length mismatch"
                    );

                    frame_count += 1;
                    total_samples += frame.sample_count as usize;
                }
                Ok(None) => {
                    println!("No more audio frames");
                    break;
                }
                Err(e) => {
                    panic!("Error decoding audio frame: {:?}", e);
                }
            }
        }

        println!("Decoded {} audio frames with {} total samples", frame_count, total_samples);
        assert!(frame_count > 0, "Should have decoded at least one audio frame");
        assert!(total_samples > 0, "Should have decoded some audio samples");
    }
}
