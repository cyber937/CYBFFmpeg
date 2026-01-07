//! Threading module for parallel decoding and prefetching

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;

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

/// Prefetch manager
pub struct PrefetchManager {
    /// Number of worker threads
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
}

impl PrefetchManager {
    /// Create a new prefetch manager
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

            let handle = thread::Builder::new()
                .name(format!("prefetch-worker-{}", i))
                .spawn(move || {
                    Self::worker_loop(command_rx, result_tx, is_running_clone);
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

    /// Worker loop
    fn worker_loop(
        command_rx: Receiver<PrefetchCommand>,
        result_tx: Sender<PrefetchResult>,
        is_running: Arc<AtomicBool>,
    ) {
        log::debug!("Prefetch worker started");

        while is_running.load(Ordering::Acquire) {
            match command_rx.recv_timeout(std::time::Duration::from_millis(100)) {
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
                    // TODO: Actual prefetch decoding
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
