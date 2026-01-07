//! Video decoder module using ffmpeg-next
//!
//! This module provides the core decoding functionality using FFmpeg.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::cache::{Cache, CacheConfig};
use crate::error::{Error, Result};

mod config;
mod ffmpeg_decoder;
mod frame;
mod info;

pub use config::{DecoderConfig, PixelFormat};
pub use frame::VideoFrame;
pub use info::{AudioTrack, CodecInfo, MediaInfo, VideoTrack};

use ffmpeg_decoder::FFmpegContext;

/// Main decoder struct
pub struct Decoder {
    /// Path to the media file
    path: String,

    /// Decoder configuration
    config: DecoderConfig,

    /// Media information (populated after prepare)
    media_info: RwLock<Option<MediaInfo>>,

    /// FFmpeg context
    ffmpeg_ctx: Mutex<Option<FFmpegContext>>,

    /// Frame cache
    cache: Arc<Cache>,

    /// Whether the decoder is prepared
    is_prepared: AtomicBool,

    /// Whether decoding is active
    is_decoding: AtomicBool,

    /// Whether prefetch is active
    is_prefetching: AtomicBool,

    /// Current playhead position in microseconds
    current_time_us: AtomicI64,

    /// Current frame number
    current_frame: AtomicI64,
}

impl Decoder {
    /// Create a new decoder
    pub fn new<P: AsRef<Path>>(path: P, config: DecoderConfig) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // Verify file exists
        if !path.as_ref().exists() {
            return Err(Error::FileNotFound(path.as_ref().to_path_buf()));
        }

        let cache_config = CacheConfig {
            l1_capacity: config.l1_cache_capacity as usize,
            l2_capacity: config.l2_cache_capacity as usize,
            l3_capacity: config.l3_cache_capacity as usize,
            enable_prefetch: config.enable_prefetch,
        };

        Ok(Self {
            path: path_str,
            config,
            media_info: RwLock::new(None),
            ffmpeg_ctx: Mutex::new(None),
            cache: Arc::new(Cache::new(cache_config)),
            is_prepared: AtomicBool::new(false),
            is_decoding: AtomicBool::new(false),
            is_prefetching: AtomicBool::new(false),
            current_time_us: AtomicI64::new(0),
            current_frame: AtomicI64::new(0),
        })
    }

    /// Prepare the decoder (loads metadata, initializes codecs)
    pub fn prepare(&self) -> Result<()> {
        if self.is_prepared.load(Ordering::Acquire) {
            return Ok(());
        }

        log::info!("Preparing decoder for: {}", self.path);

        // Initialize FFmpeg context
        let ctx = FFmpegContext::new(&self.path, &self.config)?;

        // Extract media info
        let media_info = ctx.get_media_info()?;

        // Store context and info
        {
            let mut ctx_lock = self.ffmpeg_ctx.lock();
            *ctx_lock = Some(ctx);
        }
        {
            let mut info_lock = self.media_info.write();
            *info_lock = Some(media_info);
        }

        self.is_prepared.store(true, Ordering::Release);
        log::info!("Decoder prepared successfully");

        Ok(())
    }

    /// Get media information
    pub fn media_info(&self) -> Option<MediaInfo> {
        self.media_info.read().clone()
    }

    /// Check if prepared
    pub fn is_prepared(&self) -> bool {
        self.is_prepared.load(Ordering::Acquire)
    }

    /// Check if decoding
    pub fn is_decoding(&self) -> bool {
        self.is_decoding.load(Ordering::Acquire)
    }

    /// Check if prefetching
    pub fn is_prefetching(&self) -> bool {
        self.is_prefetching.load(Ordering::Acquire)
    }

    /// Get current time in microseconds
    pub fn current_time_us(&self) -> i64 {
        self.current_time_us.load(Ordering::Acquire)
    }

    /// Start sequential decoding
    pub fn start_decoding(&self) -> Result<()> {
        if !self.is_prepared() {
            return Err(Error::NotPrepared);
        }
        self.is_decoding.store(true, Ordering::Release);
        Ok(())
    }

    /// Stop sequential decoding
    pub fn stop_decoding(&self) {
        self.is_decoding.store(false, Ordering::Release);
    }

    /// Seek to time in microseconds
    pub fn seek(&self, time_us: i64) -> Result<()> {
        if !self.is_prepared() {
            log::warn!("Decoder::seek - not prepared");
            return Err(Error::NotPrepared);
        }

        log::info!("Decoder::seek - acquiring lock for {} us", time_us);

        let mut ctx_lock = self.ffmpeg_ctx.lock();
        log::info!("Decoder::seek - lock acquired");

        if let Some(ref mut ctx) = *ctx_lock {
            log::info!("Decoder::seek - calling FFmpegContext::seek");
            ctx.seek(time_us)?;
            log::info!("Decoder::seek - seek complete, updating current_time_us");
            self.current_time_us.store(time_us, Ordering::Release);
        } else {
            log::warn!("Decoder::seek - no FFmpeg context");
        }

        log::info!("Decoder::seek - done");
        Ok(())
    }

    /// Get frame at specific time
    pub fn get_frame_at(&self, time_us: i64, tolerance_us: i64) -> Result<Option<VideoFrame>> {
        if !self.is_prepared() {
            log::warn!("Decoder::get_frame_at - not prepared");
            return Err(Error::NotPrepared);
        }

        log::info!(
            "Decoder::get_frame_at - time={} us, tolerance={} us",
            time_us,
            tolerance_us
        );

        // Check cache first
        if let Some(frame) = self.cache.get(time_us, tolerance_us) {
            log::info!("Decoder::get_frame_at - cache hit for frame at {} us", time_us);
            return Ok(Some(frame));
        }
        log::info!("Decoder::get_frame_at - cache miss, decoding from FFmpeg");

        // Decode from FFmpeg
        let mut ctx_lock = self.ffmpeg_ctx.lock();
        log::info!("Decoder::get_frame_at - lock acquired");

        if let Some(ref mut ctx) = *ctx_lock {
            // Seek if needed
            let current = self.current_time_us.load(Ordering::Acquire);
            let distance = (time_us - current).abs();

            log::info!("Decoder::get_frame_at - current={} us, distance={} us", current, distance);

            // Seek if we're too far from target
            if distance > tolerance_us * 10 {
                log::info!("Decoder::get_frame_at - distance too large, seeking");
                ctx.seek(time_us)?;
                log::info!("Decoder::get_frame_at - seek complete");
            }

            // Decode frames until we find one within tolerance
            let mut frame_count = 0;
            let max_frames = 100; // Limit to prevent infinite loop

            loop {
                if frame_count >= max_frames {
                    log::warn!("Decoder::get_frame_at - exceeded max frame count ({})", max_frames);
                    self.cache.record_miss();
                    return Ok(None);
                }

                log::trace!("Decoder::get_frame_at - decoding frame {}", frame_count);
                match ctx.decode_next_frame()? {
                    Some(frame) => {
                        frame_count += 1;
                        let frame_time = frame.pts_us;
                        log::info!("Decoder::get_frame_at - got frame at {} us (target: {} us)",
                            frame_time, time_us);

                        self.current_time_us.store(frame_time, Ordering::Release);
                        self.current_frame
                            .store(frame.frame_number, Ordering::Release);

                        // Cache the frame
                        if frame.is_keyframe {
                            self.cache.insert_l2(frame_time, frame.clone());
                        }
                        self.cache.insert_l1(frame_time, frame.clone());

                        // Check if within tolerance
                        if (frame_time - time_us).abs() <= tolerance_us {
                            log::info!("Decoder::get_frame_at - frame within tolerance, returning");
                            return Ok(Some(frame));
                        }

                        // Past target, return closest
                        if frame_time > time_us + tolerance_us {
                            log::info!("Decoder::get_frame_at - frame past target, returning");
                            return Ok(Some(frame));
                        }
                    }
                    None => {
                        // End of stream
                        log::info!("Decoder::get_frame_at - end of stream");
                        self.cache.record_miss();
                        return Ok(None);
                    }
                }
            }
        } else {
            log::warn!("Decoder::get_frame_at - no FFmpeg context");
        }

        self.cache.record_miss();
        Ok(None)
    }

    /// Get next frame in sequence
    pub fn get_next_frame(&self) -> Result<Option<VideoFrame>> {
        if !self.is_prepared() {
            return Err(Error::NotPrepared);
        }

        if !self.is_decoding() {
            return Ok(None);
        }

        let mut ctx_lock = self.ffmpeg_ctx.lock();
        if let Some(ref mut ctx) = *ctx_lock {
            if let Some(frame) = ctx.decode_next_frame()? {
                self.current_time_us.store(frame.pts_us, Ordering::Release);
                self.current_frame
                    .store(frame.frame_number, Ordering::Release);

                // Cache keyframes
                if frame.is_keyframe {
                    self.cache.insert_l2(frame.pts_us, frame.clone());
                }

                return Ok(Some(frame));
            }
        }

        Ok(None)
    }

    /// Start prefetch
    pub fn start_prefetch(&self, direction: i32, velocity: f64) -> Result<()> {
        if !self.is_prepared() {
            return Err(Error::NotPrepared);
        }

        log::debug!(
            "Starting prefetch: direction={}, velocity={}",
            direction,
            velocity
        );
        self.is_prefetching.store(true, Ordering::Release);

        // TODO: Start prefetch worker threads
        // For now, just set the flag
        Ok(())
    }

    /// Stop prefetch
    pub fn stop_prefetch(&self) {
        log::debug!("Stopping prefetch");
        self.is_prefetching.store(false, Ordering::Release);
    }

    /// Get cache statistics
    pub fn cache_statistics(&self) -> crate::cache::CacheStatistics {
        self.cache.statistics()
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Get path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get configuration
    pub fn config(&self) -> &DecoderConfig {
        &self.config
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stop_decoding();
        self.stop_prefetch();
        log::debug!("Decoder dropped for: {}", self.path);
    }
}

// Make Decoder Send + Sync safe
unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_new_missing_file() {
        let result = Decoder::new("/nonexistent/file.mp4", DecoderConfig::default());
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }
}
