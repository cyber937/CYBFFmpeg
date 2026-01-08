//! Threading module for parallel decoding and prefetching
//!
//! This module provides prefetching functionality for frame decoding.
//! Each prefetch worker has its own FFmpegContext to avoid locking issues.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;

use crate::cache::Cache;
use crate::decoder::config::DecoderConfig;
use crate::decoder::ffmpeg_decoder::FFmpegContext;

/// Prefetch command
#[derive(Debug, Clone)]
pub enum PrefetchCommand {
    /// Start prefetching
    Start {
        direction: i32,
        velocity: f64,
        current_time_us: i64,
    },
    /// Stop prefetching
    Stop,
    /// Shutdown worker
    Shutdown,
}

/// Prefetch result
#[derive(Debug)]
pub enum PrefetchResult {
    /// Frame decoded
    Frame { pts_us: i64 },
    /// Prefetch stopped
    Stopped,
    /// Error occurred
    Error(String),
}

/// Context required by prefetch workers
pub struct PrefetchContext {
    /// Path to media file
    pub path: String,

    /// Decoder configuration
    pub config: DecoderConfig,

    /// Frame cache (shared)
    pub cache: Arc<Cache>,

    /// Current playhead position (shared, updated by main decoder)
    pub current_time_us: Arc<AtomicI64>,

    /// Frame rate (for calculating frame duration)
    pub frame_rate: f64,

    /// Duration in microseconds
    pub duration_us: i64,
}

impl PrefetchContext {
    /// Create a new prefetch context
    pub fn new(
        path: String,
        config: DecoderConfig,
        cache: Arc<Cache>,
        current_time_us: Arc<AtomicI64>,
        frame_rate: f64,
        duration_us: i64,
    ) -> Self {
        Self {
            path,
            config,
            cache,
            current_time_us,
            frame_rate,
            duration_us,
        }
    }
}

/// Prefetch manager
pub struct PrefetchManager {
    /// Number of worker threads (fixed at 2)
    thread_count: usize,

    /// Command sender
    command_tx: Sender<PrefetchCommand>,

    /// Command receiver (shared by workers)
    command_rx: Receiver<PrefetchCommand>,

    /// Result sender
    result_tx: Sender<PrefetchResult>,

    /// Result receiver
    result_rx: Receiver<PrefetchResult>,

    /// Worker threads
    workers: Mutex<Vec<JoinHandle<()>>>,

    /// Whether running
    is_running: AtomicBool,

    /// Current direction
    direction: AtomicI32,

    /// Prefetch context (shared configuration)
    context: Option<Arc<PrefetchContext>>,
}

impl PrefetchManager {
    /// Create a new prefetch manager with context
    pub fn new_with_context(thread_count: usize, context: PrefetchContext) -> Arc<Self> {
        let (command_tx, command_rx) = bounded(16);
        let (result_tx, result_rx) = bounded(64);

        Arc::new(Self {
            thread_count,
            command_tx,
            command_rx,
            result_tx,
            result_rx,
            workers: Mutex::new(Vec::new()),
            is_running: AtomicBool::new(false),
            direction: AtomicI32::new(0),
            context: Some(Arc::new(context)),
        })
    }

    /// Create a new prefetch manager (for testing, without context)
    pub fn new(thread_count: usize) -> Arc<Self> {
        let (command_tx, command_rx) = bounded(16);
        let (result_tx, result_rx) = bounded(64);

        Arc::new(Self {
            thread_count,
            command_tx,
            command_rx,
            result_tx,
            result_rx,
            workers: Mutex::new(Vec::new()),
            is_running: AtomicBool::new(false),
            direction: AtomicI32::new(0),
            context: None,
        })
    }

    /// Start prefetching
    pub fn start(self: &Arc<Self>, direction: i32, velocity: f64, current_time_us: i64) {
        // Stop any existing prefetch
        self.stop();

        self.direction.store(direction, Ordering::Release);
        self.is_running.store(true, Ordering::Release);

        // Start worker threads
        let mut workers = self.workers.lock();
        for i in 0..self.thread_count {
            let command_rx = self.command_rx.clone();
            let result_tx = self.result_tx.clone();
            let is_running = Arc::new(AtomicBool::new(true));
            let is_running_clone = is_running.clone();
            let context = self.context.clone();

            let handle = thread::Builder::new()
                .name(format!("prefetch-worker-{}", i))
                .spawn(move || {
                    Self::worker_loop(command_rx, result_tx, is_running_clone, context);
                })
                .expect("Failed to spawn prefetch worker");

            workers.push(handle);
        }

        // Send start command
        let _ = self.command_tx.send(PrefetchCommand::Start {
            direction,
            velocity,
            current_time_us,
        });
    }

    /// Stop prefetching
    pub fn stop(&self) {
        if !self.is_running.load(Ordering::Acquire) {
            return;
        }

        self.is_running.store(false, Ordering::Release);
        self.direction.store(0, Ordering::Release);

        // Send stop commands
        for _ in 0..self.thread_count {
            let _ = self.command_tx.send(PrefetchCommand::Stop);
        }

        // Wait for workers (with timeout)
        let mut workers = self.workers.lock();
        for handle in workers.drain(..) {
            let _ = handle.join();
        }
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    /// Get current direction
    pub fn direction(&self) -> i32 {
        self.direction.load(Ordering::Acquire)
    }

    /// Get result receiver for polling
    pub fn results(&self) -> &Receiver<PrefetchResult> {
        &self.result_rx
    }

    /// Worker loop with actual frame decoding
    fn worker_loop(
        command_rx: Receiver<PrefetchCommand>,
        result_tx: Sender<PrefetchResult>,
        is_running: Arc<AtomicBool>,
        context: Option<Arc<PrefetchContext>>,
    ) {
        log::debug!("Prefetch worker started");

        // Create dedicated FFmpegContext for this worker
        let mut ffmpeg_ctx: Option<FFmpegContext> = None;

        while is_running.load(Ordering::Acquire) {
            match command_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(PrefetchCommand::Start {
                    direction,
                    velocity,
                    current_time_us,
                }) => {
                    log::debug!(
                        "Starting prefetch: dir={}, vel={}, time={}",
                        direction,
                        velocity,
                        current_time_us
                    );

                    // Initialize FFmpegContext if we have context
                    let ctx = match &context {
                        Some(pctx) => {
                            // Create or reuse FFmpegContext
                            if ffmpeg_ctx.is_none() {
                                match FFmpegContext::new(&pctx.path, &pctx.config) {
                                    Ok(ctx) => {
                                        ffmpeg_ctx = Some(ctx);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to create FFmpegContext for prefetch: {:?}", e);
                                        let _ = result_tx.send(PrefetchResult::Error(format!("{:?}", e)));
                                        continue;
                                    }
                                }
                            }
                            ffmpeg_ctx.as_mut().unwrap()
                        }
                        None => {
                            log::debug!("No prefetch context, skipping");
                            continue;
                        }
                    };

                    let pctx = context.as_ref().unwrap();

                    // Calculate frame duration in microseconds
                    let frame_duration_us = if pctx.frame_rate > 0.0 {
                        (1_000_000.0 / pctx.frame_rate) as i64
                    } else {
                        33333 // Default ~30fps
                    };

                    // Prefetch loop
                    let mut target_time = current_time_us;
                    let max_prefetch_frames = 30; // Limit frames per prefetch cycle
                    let mut frames_decoded = 0;

                    while is_running.load(Ordering::Acquire) && frames_decoded < max_prefetch_frames {
                        // Check for stop command (non-blocking)
                        if let Ok(cmd) = command_rx.try_recv() {
                            match cmd {
                                PrefetchCommand::Stop | PrefetchCommand::Shutdown => {
                                    log::debug!("Prefetch interrupted by command");
                                    let _ = result_tx.send(PrefetchResult::Stopped);
                                    if matches!(cmd, PrefetchCommand::Shutdown) {
                                        return;
                                    }
                                    break;
                                }
                                _ => {}
                            }
                        }

                        // Calculate next target time based on direction
                        target_time += direction as i64 * frame_duration_us;

                        // Bounds check
                        if target_time < 0 || target_time > pctx.duration_us {
                            log::debug!("Prefetch reached bounds: time={}", target_time);
                            break;
                        }

                        // Skip if already in cache
                        if pctx.cache.get(target_time, frame_duration_us / 2).is_some() {
                            log::trace!("Prefetch: cache hit for time={}", target_time);
                            continue;
                        }

                        // Seek and decode frame
                        match ctx.seek_precise(target_time) {
                            Ok(Some(frame)) => {
                                frames_decoded += 1;

                                // Insert into cache
                                if frame.is_keyframe {
                                    pctx.cache.insert_l2(frame.pts_us, frame.clone());
                                }
                                pctx.cache.insert_l3(frame.pts_us, frame.clone());

                                log::trace!(
                                    "Prefetch: decoded frame at {} us (target: {})",
                                    frame.pts_us,
                                    target_time
                                );

                                let _ = result_tx.send(PrefetchResult::Frame { pts_us: frame.pts_us });
                            }
                            Ok(None) => {
                                log::trace!("Prefetch: no frame at time={}", target_time);
                            }
                            Err(e) => {
                                log::warn!("Prefetch decode error: {:?}", e);
                                // Continue trying other frames
                            }
                        }

                        // Throttle based on velocity
                        let sleep_ms = (100.0 / velocity.abs().max(0.5)) as u64;
                        if sleep_ms > 0 && sleep_ms < 100 {
                            thread::sleep(Duration::from_millis(sleep_ms));
                        }
                    }

                    log::debug!("Prefetch cycle complete: decoded {} frames", frames_decoded);
                }
                Ok(PrefetchCommand::Stop) => {
                    log::debug!("Prefetch stop received");
                    let _ = result_tx.send(PrefetchResult::Stopped);
                    break;
                }
                Ok(PrefetchCommand::Shutdown) => {
                    log::debug!("Prefetch shutdown received");
                    break;
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Continue waiting
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        log::debug!("Prefetch worker stopped");
    }
}

impl Drop for PrefetchManager {
    fn drop(&mut self) {
        // Send shutdown to all workers
        for _ in 0..self.thread_count {
            let _ = self.command_tx.send(PrefetchCommand::Shutdown);
        }

        // Join all workers
        let mut workers = self.workers.lock();
        for handle in workers.drain(..) {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefetch_manager_creation() {
        let manager = PrefetchManager::new(4);
        assert!(!manager.is_running());
        assert_eq!(manager.direction(), 0);
    }

    #[test]
    fn test_prefetch_start_stop() {
        let manager = PrefetchManager::new(2);

        manager.start(1, 1.0, 0);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(manager.is_running());

        manager.stop();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!manager.is_running());
    }
}
