//! Video decoder module using ffmpeg-next
//!
//! This module provides the core decoding functionality using FFmpeg.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::cache::{Cache, CacheConfig};
use crate::error::{Error, Result};
use crate::threading::{PrefetchContext, PrefetchManager};

mod audio_frame;
pub(crate) mod config;
pub(crate) mod ffmpeg_decoder;
mod frame;
mod info;

pub use audio_frame::{AudioFrame, SampleFormat};
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

    /// Current playhead position in microseconds (shared with prefetch workers)
    current_time_us: Arc<AtomicI64>,

    /// Current frame number
    current_frame: AtomicI64,

    /// Prefetch manager (created on first start_prefetch call)
    prefetch_manager: Mutex<Option<Arc<PrefetchManager>>>,
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
            current_time_us: Arc::new(AtomicI64::new(0)),
            current_frame: AtomicI64::new(0),
            prefetch_manager: Mutex::new(None),
        })
    }

    /// Prepare the decoder (loads metadata, initializes codecs)
    pub fn prepare(&self) -> Result<()> {
        if self.is_prepared.load(Ordering::Acquire) {
            return Ok(());
        }

        log::info!("Preparing decoder for: {}", self.path);

        // Initialize FFmpeg context
        let mut ctx = FFmpegContext::new(&self.path, &self.config)?;

        // Extract media info
        let media_info = ctx.get_media_info()?;

        // Build keyframe index for fast seeking (synchronous during prepare)
        // Limit to 2000 entries to prevent excessive memory usage on very long videos
        let keyframe_count = ctx.build_keyframe_index(2000).unwrap_or_else(|e| {
            log::warn!("Failed to build keyframe index: {:?}", e);
            0
        });
        if keyframe_count > 0 {
            log::info!("Built keyframe index with {} entries", keyframe_count);
        }

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

    /// Seek to time in microseconds (keyframe seek)
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

    /// Seek precisely to time in microseconds (frame-accurate seek).
    /// This performs a keyframe seek first, then decodes frames until reaching the target time.
    /// Returns the frame at or just before the target time.
    pub fn seek_precise(&self, time_us: i64) -> Result<Option<VideoFrame>> {
        if !self.is_prepared() {
            log::warn!("Decoder::seek_precise - not prepared");
            return Err(Error::NotPrepared);
        }

        log::info!("Decoder::seek_precise - acquiring lock for {} us", time_us);

        let mut ctx_lock = self.ffmpeg_ctx.lock();
        log::info!("Decoder::seek_precise - lock acquired");

        if let Some(ref mut ctx) = *ctx_lock {
            log::info!("Decoder::seek_precise - calling FFmpegContext::seek_precise");
            let frame = ctx.seek_precise(time_us)?;

            if let Some(ref f) = frame {
                log::info!(
                    "Decoder::seek_precise - got frame at {} us, updating current_time_us",
                    f.pts_us
                );
                self.current_time_us.store(f.pts_us, Ordering::Release);
                self.current_frame.store(f.frame_number, Ordering::Release);

                // Cache the frame
                if f.is_keyframe {
                    self.cache.insert_l2(f.pts_us, f.clone());
                }
                self.cache.insert_l1(f.pts_us, f.clone());
            } else {
                log::warn!("Decoder::seek_precise - no frame returned");
            }

            log::info!("Decoder::seek_precise - done");
            return Ok(frame);
        } else {
            log::warn!("Decoder::seek_precise - no FFmpeg context");
        }

        Ok(None)
    }

    /// Prime the audio decoder after seek.
    /// Call this after seek() and before get_next_audio_frame() to ensure
    /// audio packets are pre-loaded into the queue for immediate decoding.
    /// This is necessary because after seek, the first packets read from the
    /// stream may be video packets, leaving the audio queue empty.
    /// Returns the number of audio packets that were queued.
    pub fn prime_audio_after_seek(&self) -> Result<u32> {
        if !self.is_prepared() {
            log::warn!("Decoder::prime_audio_after_seek - not prepared");
            return Err(Error::NotPrepared);
        }

        log::info!("Decoder::prime_audio_after_seek - acquiring lock");

        let mut ctx_lock = self.ffmpeg_ctx.lock();

        if let Some(ref mut ctx) = *ctx_lock {
            log::info!("Decoder::prime_audio_after_seek - calling FFmpegContext::prime_audio_after_seek");
            let count = ctx.prime_audio_after_seek()?;
            log::info!("Decoder::prime_audio_after_seek - done, queued {} audio packets", count);
            Ok(count)
        } else {
            log::warn!("Decoder::prime_audio_after_seek - no FFmpeg context");
            Ok(0)
        }
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

    /// Get next audio frame in sequence
    pub fn get_next_audio_frame(&self) -> Result<Option<AudioFrame>> {
        if !self.is_prepared() {
            return Err(Error::NotPrepared);
        }

        if !self.is_decoding() {
            return Ok(None);
        }

        let mut ctx_lock = self.ffmpeg_ctx.lock();
        if let Some(ref mut ctx) = *ctx_lock {
            if let Some(frame) = ctx.decode_next_audio_frame()? {
                return Ok(Some(frame));
            }
        }

        Ok(None)
    }

    /// Check if media has audio
    pub fn has_audio(&self) -> bool {
        if let Some(ref info) = *self.media_info.read() {
            return !info.audio_tracks.is_empty();
        }
        false
    }

    /// Get audio sample rate
    pub fn audio_sample_rate(&self) -> u32 {
        let ctx_lock = self.ffmpeg_ctx.lock();
        if let Some(ref ctx) = *ctx_lock {
            return ctx.audio_sample_rate();
        }
        0
    }

    /// Get audio channels
    pub fn audio_channels(&self) -> u32 {
        let ctx_lock = self.ffmpeg_ctx.lock();
        if let Some(ref ctx) = *ctx_lock {
            return ctx.audio_channels();
        }
        0
    }

    /// Start prefetch
    ///
    /// Starts background prefetching of frames in the specified direction.
    /// This creates or reuses a PrefetchManager with 2 worker threads that
    /// decode frames ahead of the current position for smooth scrubbing.
    ///
    /// # Arguments
    /// * `direction` - Prefetch direction (1 = forward, -1 = backward)
    /// * `velocity` - Scrub velocity in x speed (1.0 = normal speed)
    pub fn start_prefetch(&self, direction: i32, velocity: f64) -> Result<()> {
        if !self.is_prepared() {
            return Err(Error::NotPrepared);
        }

        // Skip if prefetch is disabled in config
        if !self.config.enable_prefetch {
            log::debug!("Prefetch disabled in config, skipping");
            return Ok(());
        }

        log::info!(
            "Starting prefetch: direction={}, velocity={}",
            direction,
            velocity
        );

        // Get media info for frame rate and duration
        let (frame_rate, duration_us) = {
            let info = self.media_info.read();
            if let Some(ref info) = *info {
                let fr = info.primary_video().map(|v| v.frame_rate).unwrap_or(30.0);
                // Convert duration from seconds to microseconds
                let dur = (info.duration * 1_000_000.0) as i64;
                (fr, dur)
            } else {
                (30.0, 0)
            }
        };

        // Create or get prefetch manager
        let mut pm_lock = self.prefetch_manager.lock();
        let manager = if pm_lock.is_none() {
            // Create new prefetch context
            let context = PrefetchContext::new(
                self.path.clone(),
                self.config.clone(),
                self.cache.clone(),
                self.current_time_us.clone(),
                frame_rate,
                duration_us,
            );

            // Create manager with 2 threads (as specified in plan)
            let manager = PrefetchManager::new_with_context(2, context);
            *pm_lock = Some(manager.clone());
            manager
        } else {
            pm_lock.as_ref().unwrap().clone()
        };

        // Start prefetching
        let current = self.current_time_us.load(Ordering::Acquire);
        manager.start(direction, velocity, current);

        self.is_prefetching.store(true, Ordering::Release);
        Ok(())
    }

    /// Stop prefetch
    ///
    /// Stops any running prefetch workers. The PrefetchManager is retained
    /// for potential reuse on the next start_prefetch call.
    pub fn stop_prefetch(&self) {
        log::info!("Stopping prefetch");

        // Stop the prefetch manager if active
        if let Some(ref manager) = *self.prefetch_manager.lock() {
            manager.stop();
        }

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
