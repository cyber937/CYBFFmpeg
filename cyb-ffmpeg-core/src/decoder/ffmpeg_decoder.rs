//! FFmpeg decoder implementation using ffmpeg-next
//!
//! This module provides the actual FFmpeg integration via ffmpeg-next bindings.

use std::collections::HashMap;
use std::path::Path;

use ffmpeg_next as ffmpeg;
use ffmpeg_next::codec::context::Context as CodecContext;
use ffmpeg_next::format::context::Input as FormatContext;
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::software::scaling::{Context as ScalerContext, Flags as ScalerFlags};
use ffmpeg_next::util::frame::video::Video as VideoFrameFFmpeg;
use ffmpeg_next::Rational;

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

    /// Scaler for pixel format conversion
    scaler: Option<ScalerContext>,

    /// Target pixel format
    target_format: PixelFormat,

    /// Frame counter
    frame_number: i64,

    /// Time base for video stream
    video_time_base: Rational,

    /// Video duration in microseconds
    duration_us: i64,

    /// Frame rate
    frame_rate: f64,

    /// Video width
    width: u32,

    /// Video height
    height: u32,

    /// Prefer hardware decoding
    prefer_hw: bool,
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
            scaler: None,
            target_format: config.output_pixel_format,
            frame_number: 0,
            video_time_base: Rational::new(1, 1000000),
            duration_us: 0,
            frame_rate: 0.0,
            width: 0,
            height: 0,
            prefer_hw: config.prefer_hardware_decoding,
        };

        // Initialize video decoder if we have a video stream
        if let Some(stream_idx) = ctx.video_stream_index {
            ctx.init_video_decoder(stream_idx, config)?;
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
        let duration = stream.duration();
        if duration > 0 {
            self.duration_us = Self::pts_to_us(duration, self.video_time_base);
        } else {
            // Use container duration
            let container_duration = self.input.duration();
            self.duration_us = container_duration * 1_000_000 / ffmpeg::ffi::AV_TIME_BASE as i64;
        }

        // Get frame rate
        let frame_rate = stream.avg_frame_rate();
        if frame_rate.denominator() > 0 {
            self.frame_rate = frame_rate.numerator() as f64 / frame_rate.denominator() as f64;
        } else {
            self.frame_rate = 24.0; // Default
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
        let video_decoder = decoder_ctx.decoder().video().map_err(|e| {
            Error::DecodeFailed(format!("Failed to open video decoder: {}", e))
        })?;

        self.width = video_decoder.width();
        self.height = video_decoder.height();

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

    /// Seek to a specific time in microseconds
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

        self.input
            .seek(time_us, ..time_us)
            .map_err(|e| {
                log::error!("FFmpegContext::seek - seek failed: {}", e);
                Error::SeekFailed(time_us)
            })?;
        log::info!("FFmpegContext::seek - seek succeeded");

        // Flush decoder buffers - critical after seek!
        if let Some(ref mut decoder) = self.video_decoder {
            log::info!("FFmpegContext::seek - flushing decoder");
            decoder.flush();
        }

        // Reset frame counter for accurate tracking after seek
        self.frame_number = 0;

        log::info!("FFmpegContext::seek - complete");
        Ok(())
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

            log::trace!("decode_next_frame - reading packet {}", packet_count);

            // Read packets until we get a video packet
            let packet = match self.input.packets().next() {
                Some((stream, packet)) => {
                    packet_count += 1;
                    if stream.index() == video_stream_idx {
                        log::trace!("decode_next_frame - got video packet {}", packet_count);
                        Some(packet)
                    } else {
                        log::trace!("decode_next_frame - skipping non-video packet (stream {})", stream.index());
                        continue; // Skip non-video packets
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
}
