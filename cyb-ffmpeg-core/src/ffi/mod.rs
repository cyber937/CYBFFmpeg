//! FFI exports for C/Swift interop
//!
//! All functions in this module are exported with `#[no_mangle]`
//! and use C-compatible types for cross-language interop.

use std::ffi::{c_char, CStr, CString};
use std::ptr;

use parking_lot::Mutex;

use crate::cache::CacheStatistics;
use crate::decoder::{
    AudioFrame, Decoder, DecoderConfig, FrameRate, MediaInfo, PixelFormat, Timecode,
    TimecodeSourceKind, VideoFrame,
};
use crate::error::Error;

// Thread-local error storage
thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> = std::cell::RefCell::new(None);
}

fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(msg).ok();
    });
}

// =============================================================================
// Result Type
// =============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CybResult {
    Success = 0,
    ErrorFileNotFound = 1,
    ErrorInvalidFormat = 2,
    ErrorCodecNotSupported = 3,
    ErrorDecodeFailed = 4,
    ErrorSeekFailed = 5,
    ErrorMemory = 6,
    ErrorInvalidHandle = 7,
    ErrorNotPrepared = 8,
    ErrorUnknown = 99,
}

impl From<Error> for CybResult {
    fn from(e: Error) -> Self {
        set_last_error(&e.to_string());
        match e {
            Error::FileNotFound(_) => CybResult::ErrorFileNotFound,
            Error::InvalidFormat(_) => CybResult::ErrorInvalidFormat,
            Error::CodecNotSupported(_) => CybResult::ErrorCodecNotSupported,
            Error::DecodeFailed(_) => CybResult::ErrorDecodeFailed,
            Error::SeekFailed(_) => CybResult::ErrorSeekFailed,
            Error::Memory => CybResult::ErrorMemory,
            Error::InvalidHandle => CybResult::ErrorInvalidHandle,
            Error::NotPrepared => CybResult::ErrorNotPrepared,
            _ => CybResult::ErrorUnknown,
        }
    }
}

impl<T> From<Result<T, Error>> for CybResult {
    fn from(r: Result<T, Error>) -> Self {
        match r {
            Ok(_) => CybResult::Success,
            Err(e) => e.into(),
        }
    }
}

// =============================================================================
// Opaque Handle
// =============================================================================

/// Opaque decoder handle
pub struct CybDecoderHandle {
    decoder: Mutex<Decoder>,
}

// =============================================================================
// Error Handling
// =============================================================================

/// Get last error message
#[no_mangle]
pub extern "C" fn cyb_get_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()))
}

/// Clear last error
#[no_mangle]
pub extern "C" fn cyb_clear_last_error() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}

/// Initialize the library (sets up logging, etc.)
/// Call once at application startup.
#[no_mangle]
pub extern "C" fn cyb_init() {
    crate::init();
}

// =============================================================================
// Configuration Types (FFI)
// =============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CybCacheConfig {
    pub l1_capacity: u32,
    pub l2_capacity: u32,
    pub l3_capacity: u32,
    pub enable_prefetch: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CybDecoderConfig {
    pub prefer_hardware_decoding: bool,
    pub cache_config: CybCacheConfig,
    pub thread_count: u32,
    pub output_pixel_format: u8, // 0=BGRA, 1=NV12, 2=YUV420P
    /// When true, `cyb_decoder_prepare` skips building the keyframe index.
    /// Suitable for callers that only need metadata/timecode and will not
    /// seek or play back. Default behavior (`false`) is preserved.
    pub skip_keyframe_indexing: bool,
    /// Optional UTF-8, NUL-terminated path to a disk-cache file for the
    /// keyframe index. Pass NULL to disable caching (default behavior:
    /// rebuild every time). When set, `prepare()` first tries to load a
    /// previously-saved index from this path; on miss/stale, it builds
    /// the index normally and saves it back. The pointer only needs to
    /// be valid for the duration of the `cyb_decoder_create` call —
    /// Rust copies the string into its own allocation.
    pub keyframe_index_cache_path: *const c_char,
}

impl From<&CybDecoderConfig> for DecoderConfig {
    fn from(c: &CybDecoderConfig) -> Self {
        let cache_path = if c.keyframe_index_cache_path.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(c.keyframe_index_cache_path) }
                .to_str()
                .ok()
                .map(|s| s.to_owned())
        };

        DecoderConfig {
            prefer_hardware_decoding: c.prefer_hardware_decoding,
            l1_cache_capacity: c.cache_config.l1_capacity,
            l2_cache_capacity: c.cache_config.l2_capacity,
            l3_cache_capacity: c.cache_config.l3_capacity,
            enable_prefetch: c.cache_config.enable_prefetch,
            thread_count: c.thread_count,
            output_pixel_format: match c.output_pixel_format {
                0 => PixelFormat::Bgra,
                1 => PixelFormat::Nv12,
                _ => PixelFormat::Yuv420p,
            },
            skip_keyframe_indexing: c.skip_keyframe_indexing,
            keyframe_index_cache_path: cache_path,
        }
    }
}

// =============================================================================
// Decoder Lifecycle
// =============================================================================

/// Create decoder
#[no_mangle]
pub extern "C" fn cyb_decoder_create(
    path: *const c_char,
    config: *const CybDecoderConfig,
) -> *mut CybDecoderHandle {
    if path.is_null() {
        set_last_error("Path is null");
        return ptr::null_mut();
    }

    let path_str = unsafe {
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("Invalid UTF-8 in path");
                return ptr::null_mut();
            }
        }
    };

    let decoder_config = if config.is_null() {
        DecoderConfig::default()
    } else {
        unsafe { DecoderConfig::from(&*config) }
    };

    match Decoder::new(path_str, decoder_config) {
        Ok(decoder) => Box::into_raw(Box::new(CybDecoderHandle {
            decoder: Mutex::new(decoder),
        })),
        Err(e) => {
            set_last_error(&e.to_string());
            ptr::null_mut()
        }
    }
}

/// Prepare decoder
#[no_mangle]
pub extern "C" fn cyb_decoder_prepare(handle: *mut CybDecoderHandle) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }

    let handle = unsafe { &*handle };
    let mut decoder = handle.decoder.lock();
    decoder.prepare().into()
}

/// Discard all video packets at the demuxer. Audio-only callers can
/// call this after `prepare()` to skip the I/O cost of reading video
/// bytes that will never be decoded — empirically 5-10× speedup for
/// audio extraction on 4K MXF where video dominates the file size.
/// No-op when the source has no video stream.
#[no_mangle]
pub extern "C" fn cyb_decoder_discard_video_stream(
    handle: *mut CybDecoderHandle,
) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    let decoder = handle.decoder.lock();
    decoder.discard_video_stream().into()
}

/// Destroy decoder
#[no_mangle]
pub extern "C" fn cyb_decoder_destroy(handle: *mut CybDecoderHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

/// Check if prepared
#[no_mangle]
pub extern "C" fn cyb_decoder_is_prepared(handle: *const CybDecoderHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().is_prepared()
}

// =============================================================================
// Cache Statistics
// =============================================================================

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CybCacheStats {
    pub l1_entries: u32,
    pub l2_entries: u32,
    pub l3_entries: u32,
    pub l1_hit_count: u64,
    pub l2_hit_count: u64,
    pub l3_hit_count: u64,
    pub miss_count: u64,
    pub memory_usage_bytes: u64,
}

impl From<CacheStatistics> for CybCacheStats {
    fn from(s: CacheStatistics) -> Self {
        Self {
            l1_entries: s.l1_entries as u32,
            l2_entries: s.l2_entries as u32,
            l3_entries: s.l3_entries as u32,
            l1_hit_count: s.l1_hit_count,
            l2_hit_count: s.l2_hit_count,
            l3_hit_count: s.l3_hit_count,
            miss_count: s.miss_count,
            memory_usage_bytes: s.memory_usage_bytes,
        }
    }
}

/// Get cache statistics
#[no_mangle]
pub extern "C" fn cyb_decoder_get_cache_stats(
    handle: *const CybDecoderHandle,
    out_stats: *mut CybCacheStats,
) {
    if handle.is_null() || out_stats.is_null() {
        return;
    }

    let handle = unsafe { &*handle };
    let stats = handle.decoder.lock().cache_statistics();

    unsafe {
        *out_stats = stats.into();
    }
}

// =============================================================================
// Version Info
// =============================================================================

static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
static FFMPEG_VERSION: &str = "7.0\0"; // Placeholder

/// Get library version
#[no_mangle]
pub extern "C" fn cyb_get_version() -> *const c_char {
    VERSION.as_ptr() as *const c_char
}

/// Get FFmpeg version
#[no_mangle]
pub extern "C" fn cyb_get_ffmpeg_version() -> *const c_char {
    FFMPEG_VERSION.as_ptr() as *const c_char
}

// =============================================================================
// Decoding (Stub implementations)
// =============================================================================

/// Start decoding
#[no_mangle]
pub extern "C" fn cyb_decoder_start(handle: *mut CybDecoderHandle) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().start_decoding().into()
}

/// Stop decoding
#[no_mangle]
pub extern "C" fn cyb_decoder_stop(handle: *mut CybDecoderHandle) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().stop_decoding();
    CybResult::Success
}

/// Seek (keyframe seek - fast but may not be frame-accurate)
#[no_mangle]
pub extern "C" fn cyb_decoder_seek(handle: *mut CybDecoderHandle, time_us: i64) -> CybResult {
    log::info!("FFI::cyb_decoder_seek - time_us={}", time_us);
    if handle.is_null() {
        log::warn!("FFI::cyb_decoder_seek - handle is null");
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    log::info!("FFI::cyb_decoder_seek - acquiring lock");
    let result: CybResult = handle.decoder.lock().seek(time_us).into();
    log::info!("FFI::cyb_decoder_seek - done, result={:?}", result);
    result
}

/// Seek precisely (frame-accurate seek).
/// This performs a keyframe seek first, then decodes frames until reaching the target time.
/// Returns the frame at or just before the target time.
#[no_mangle]
pub extern "C" fn cyb_decoder_seek_precise(
    handle: *mut CybDecoderHandle,
    time_us: i64,
    out_frame: *mut *mut CybFrameHandle,
) -> CybResult {
    log::info!("FFI::cyb_decoder_seek_precise - time_us={}", time_us);
    if handle.is_null() || out_frame.is_null() {
        log::warn!("FFI::cyb_decoder_seek_precise - handle or out_frame is null");
        return CybResult::ErrorInvalidHandle;
    }

    let handle = unsafe { &*handle };
    log::info!("FFI::cyb_decoder_seek_precise - acquiring lock");
    let decoder = handle.decoder.lock();
    log::info!("FFI::cyb_decoder_seek_precise - lock acquired, calling seek_precise");

    match decoder.seek_precise(time_us) {
        Ok(Some(frame)) => {
            log::info!(
                "FFI::cyb_decoder_seek_precise - got frame: pts={} us, {}x{}",
                frame.pts_us,
                frame.width,
                frame.height
            );
            let frame_handle = Box::new(CybFrameHandle { frame });
            unsafe {
                *out_frame = Box::into_raw(frame_handle);
            }
            CybResult::Success
        }
        Ok(None) => {
            log::info!("FFI::cyb_decoder_seek_precise - no frame");
            unsafe {
                *out_frame = ptr::null_mut();
            }
            CybResult::Success
        }
        Err(e) => {
            log::error!("FFI::cyb_decoder_seek_precise - error: {:?}", e);
            e.into()
        }
    }
}

/// Prime audio decoder after seek.
/// Call this after seek and before reading audio frames to ensure
/// audio packets are pre-loaded into the queue for immediate decoding.
/// Returns the number of audio packets queued, or 0 if no audio.
#[no_mangle]
pub extern "C" fn cyb_decoder_prime_audio_after_seek(handle: *mut CybDecoderHandle) -> u32 {
    log::info!("FFI::cyb_decoder_prime_audio_after_seek");
    if handle.is_null() {
        log::warn!("FFI::cyb_decoder_prime_audio_after_seek - handle is null");
        return 0;
    }
    let handle = unsafe { &*handle };
    match handle.decoder.lock().prime_audio_after_seek() {
        Ok(count) => {
            log::info!("FFI::cyb_decoder_prime_audio_after_seek - done, queued {} packets", count);
            count
        }
        Err(e) => {
            log::error!("FFI::cyb_decoder_prime_audio_after_seek - error: {:?}", e);
            set_last_error(&e.to_string());
            0
        }
    }
}

/// Get current time
#[no_mangle]
pub extern "C" fn cyb_decoder_get_current_time(handle: *const CybDecoderHandle) -> i64 {
    if handle.is_null() {
        return 0;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().current_time_us()
}

/// Start prefetch
#[no_mangle]
pub extern "C" fn cyb_decoder_start_prefetch(
    handle: *mut CybDecoderHandle,
    direction: i32,
    velocity: f64,
) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    handle
        .decoder
        .lock()
        .start_prefetch(direction, velocity)
        .into()
}

/// Stop prefetch
#[no_mangle]
pub extern "C" fn cyb_decoder_stop_prefetch(handle: *mut CybDecoderHandle) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().stop_prefetch();
    CybResult::Success
}

/// Check if prefetching
#[no_mangle]
pub extern "C" fn cyb_decoder_is_prefetching(handle: *const CybDecoderHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().is_prefetching()
}

/// Clear cache
#[no_mangle]
pub extern "C" fn cyb_decoder_clear_cache(handle: *mut CybDecoderHandle) -> CybResult {
    if handle.is_null() {
        return CybResult::ErrorInvalidHandle;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().clear_cache();
    CybResult::Success
}

// =============================================================================
// Frame Types
// =============================================================================

/// Video frame data for FFI
#[repr(C)]
pub struct CybVideoFrame {
    /// Raw pixel data pointer (null when `cv_pixel_buffer_ptr != 0`)
    pub data: *const u8,
    /// Data size in bytes
    pub data_size: usize,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Stride (bytes per row)
    pub stride: u32,
    /// Presentation timestamp in microseconds
    pub pts_us: i64,
    /// Duration in microseconds
    pub duration_us: i64,
    /// Whether this is a keyframe
    pub is_keyframe: bool,
    /// Sequential frame number
    pub frame_number: i64,
    /// Pixel format (0=BGRA, 1=NV12, 2=YUV420P)
    pub pixel_format: u8,
    /// CVPixelBufferRef for VideoToolbox HW-decoded frames (0 = SW path).
    /// Caller takes ownership of one CFRetain.
    pub cv_pixel_buffer_ptr: u64,
}

/// Opaque frame handle (owns the data)
pub struct CybFrameHandle {
    frame: VideoFrame,
}

/// Get frame at specific time
#[no_mangle]
pub extern "C" fn cyb_decoder_get_frame_at(
    handle: *mut CybDecoderHandle,
    time_us: i64,
    tolerance_us: i64,
    out_frame: *mut *mut CybFrameHandle,
) -> CybResult {
    log::info!("FFI::cyb_decoder_get_frame_at - time_us={}, tolerance_us={}", time_us, tolerance_us);
    if handle.is_null() || out_frame.is_null() {
        log::warn!("FFI::cyb_decoder_get_frame_at - handle or out_frame is null");
        return CybResult::ErrorInvalidHandle;
    }

    let handle = unsafe { &*handle };
    log::info!("FFI::cyb_decoder_get_frame_at - acquiring lock");
    let mut decoder = handle.decoder.lock();
    log::info!("FFI::cyb_decoder_get_frame_at - lock acquired, calling get_frame_at");

    match decoder.get_frame_at(time_us, tolerance_us) {
        Ok(Some(frame)) => {
            log::info!("FFI::cyb_decoder_get_frame_at - got frame: pts={} us, {}x{}",
                frame.pts_us, frame.width, frame.height);
            let frame_handle = Box::new(CybFrameHandle { frame });
            unsafe {
                *out_frame = Box::into_raw(frame_handle);
            }
            CybResult::Success
        }
        Ok(None) => {
            log::info!("FFI::cyb_decoder_get_frame_at - no frame");
            unsafe {
                *out_frame = ptr::null_mut();
            }
            CybResult::Success
        }
        Err(e) => {
            log::error!("FFI::cyb_decoder_get_frame_at - error: {:?}", e);
            e.into()
        }
    }
}

/// Get next frame in sequence
#[no_mangle]
pub extern "C" fn cyb_decoder_get_next_frame(
    handle: *mut CybDecoderHandle,
    out_frame: *mut *mut CybFrameHandle,
) -> CybResult {
    if handle.is_null() || out_frame.is_null() {
        return CybResult::ErrorInvalidHandle;
    }

    let handle = unsafe { &*handle };
    let mut decoder = handle.decoder.lock();

    match decoder.get_next_frame() {
        Ok(Some(frame)) => {
            let frame_handle = Box::new(CybFrameHandle { frame });
            unsafe {
                *out_frame = Box::into_raw(frame_handle);
            }
            CybResult::Success
        }
        Ok(None) => {
            unsafe {
                *out_frame = ptr::null_mut();
            }
            CybResult::Success
        }
        Err(e) => e.into(),
    }
}

/// Get frame data from handle
#[no_mangle]
pub extern "C" fn cyb_frame_get_data(
    frame_handle: *const CybFrameHandle,
    out_frame: *mut CybVideoFrame,
) {
    if frame_handle.is_null() || out_frame.is_null() {
        return;
    }

    let frame_handle = unsafe { &*frame_handle };
    let frame = &frame_handle.frame;

    unsafe {
        (*out_frame).data = frame.data_ptr();
        (*out_frame).data_size = frame.data_size();
        (*out_frame).width = frame.width;
        (*out_frame).height = frame.height;
        (*out_frame).stride = frame.stride;
        (*out_frame).pts_us = frame.pts_us;
        (*out_frame).duration_us = frame.duration_us;
        (*out_frame).is_keyframe = frame.is_keyframe;
        (*out_frame).frame_number = frame.frame_number;
        (*out_frame).pixel_format = frame.pixel_format as u8;
        (*out_frame).cv_pixel_buffer_ptr = frame.cv_pixel_buffer_ptr;
    }
}

/// Release frame handle.
///
/// Swift consumes the `cv_pixel_buffer_ptr`'s `+1` retain via
/// `Unmanaged::takeRetainedValue()` in `cyb_frame_get_data`'s caller before
/// reaching this point. Zero the pointer here so `VideoFrame::drop` (which
/// would otherwise `CFRelease` the same retain) becomes a no-op for that
/// field. The cache holds clones with their own retains (added by `Clone`)
/// — those are released independently when the cache evicts them.
#[no_mangle]
pub extern "C" fn cyb_frame_release(frame_handle: *mut CybFrameHandle) {
    if !frame_handle.is_null() {
        unsafe {
            let mut boxed = Box::from_raw(frame_handle);
            boxed.frame.cv_pixel_buffer_ptr = 0;
            drop(boxed);
        }
    }
}

// =============================================================================
// Media Info Types
// =============================================================================

/// Exact frame rate as a rational number (`num / den`). Mirrors
/// [`crate::decoder::FrameRate`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CybFrameRate {
    pub num: u32,
    pub den: u32,
}

/// Source kind for an embedded SMPTE timecode. Mirrors
/// [`crate::decoder::TimecodeSourceKind`].
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum CybTimecodeSourceKind {
    TmcdTrack = 0,
    MxfMaterialPackage = 1,
    MxfSourcePackage = 2,
    ContainerMetadata = 3,
    Inferred = 4,
    Unknown = 5,
}

/// Embedded SMPTE timecode. Mirrors [`crate::decoder::Timecode`].
///
/// `source_detail` points into a CString stored inside the owning
/// `CybMediaInfoHandle`; do not free or outlive the handle.
#[repr(C)]
pub struct CybTimecode {
    /// Absolute frame number (canonical lossless representation).
    pub frame_number: i64,
    /// Exact frame rate.
    pub rate: CybFrameRate,
    /// True if drop-frame timecode (29.97 / 59.94 family).
    pub drop_frame: bool,
    /// Where the timecode was sourced from.
    pub source_kind: CybTimecodeSourceKind,
    /// Free-form provenance string (e.g., "ffmpeg:mxf:format:timecode").
    pub source_detail: *const c_char,
    /// Confidence in the value, 0.0..=1.0.
    pub confidence: f32,
}

/// Video track info for FFI
#[repr(C)]
pub struct CybVideoTrack {
    pub index: i32,
    pub codec_name: *const c_char,
    pub codec_long_name: *const c_char,
    pub width: i32,
    pub height: i32,
    /// Approximate frame rate, lossy for NTSC family. Use `frame_rate_exact`
    /// when `has_frame_rate_exact` is true for a lossless rational.
    pub frame_rate: f64,
    pub bit_rate: i64,
    pub is_hardware_decodable: bool,
    /// Whether `frame_rate_exact` carries a meaningful value.
    pub has_frame_rate_exact: bool,
    /// Lossless rational frame rate (24000/1001 etc.). Only valid when
    /// `has_frame_rate_exact` is true.
    pub frame_rate_exact: CybFrameRate,
    /// Whether `start_timecode` carries a meaningful value.
    pub has_start_timecode: bool,
    /// Embedded SMPTE timecode. Only valid when `has_start_timecode` is true.
    pub start_timecode: CybTimecode,
}

/// Audio track info for FFI
#[repr(C)]
pub struct CybAudioTrack {
    pub index: i32,
    pub codec_name: *const c_char,
    pub codec_long_name: *const c_char,
    pub sample_rate: i32,
    pub channels: i32,
    pub bit_rate: i64,
}

/// Media info for FFI
#[repr(C)]
pub struct CybMediaInfo {
    pub duration: f64,
    pub container_format: *const c_char,
    pub video_track_count: i32,
    pub audio_track_count: i32,
}

/// Opaque media info handle
pub struct CybMediaInfoHandle {
    info: MediaInfo,
    container_format_cstr: CString,
    codec_names: Vec<CString>,
    codec_long_names: Vec<CString>,
    /// Owned source-detail strings, one per video track (empty CString when
    /// the track has no embedded timecode). Indexed parallel to
    /// `info.video_tracks`.
    timecode_source_details: Vec<CString>,
}

/// Get media info
#[no_mangle]
pub extern "C" fn cyb_decoder_get_media_info(
    handle: *const CybDecoderHandle,
    out_info: *mut *mut CybMediaInfoHandle,
) -> CybResult {
    if handle.is_null() || out_info.is_null() {
        return CybResult::ErrorInvalidHandle;
    }

    let handle = unsafe { &*handle };
    let decoder = handle.decoder.lock();

    match decoder.media_info() {
        Some(info) => {
            let container_format_cstr = CString::new(info.container_format.clone())
                .unwrap_or_else(|_| CString::new("unknown").unwrap());

            // Pre-allocate string storage
            let mut codec_names = Vec::new();
            let mut codec_long_names = Vec::new();
            let mut timecode_source_details = Vec::new();

            for track in &info.video_tracks {
                codec_names.push(
                    CString::new(track.codec.name.clone())
                        .unwrap_or_else(|_| CString::new("").unwrap()),
                );
                codec_long_names.push(
                    CString::new(track.codec.long_name.clone())
                        .unwrap_or_else(|_| CString::new("").unwrap()),
                );
                let detail = track
                    .start_timecode
                    .as_ref()
                    .map(|tc| tc.source_detail.clone())
                    .unwrap_or_default();
                timecode_source_details.push(
                    CString::new(detail).unwrap_or_else(|_| CString::new("").unwrap()),
                );
            }

            for track in &info.audio_tracks {
                codec_names.push(
                    CString::new(track.codec.name.clone())
                        .unwrap_or_else(|_| CString::new("").unwrap()),
                );
                codec_long_names.push(
                    CString::new(track.codec.long_name.clone())
                        .unwrap_or_else(|_| CString::new("").unwrap()),
                );
            }

            let info_handle = Box::new(CybMediaInfoHandle {
                info,
                container_format_cstr,
                codec_names,
                codec_long_names,
                timecode_source_details,
            });

            unsafe {
                *out_info = Box::into_raw(info_handle);
            }
            CybResult::Success
        }
        None => {
            set_last_error("Media info not available (decoder not prepared)");
            CybResult::ErrorNotPrepared
        }
    }
}

/// Get media info details
#[no_mangle]
pub extern "C" fn cyb_media_info_get_details(
    info_handle: *const CybMediaInfoHandle,
    out_info: *mut CybMediaInfo,
) {
    if info_handle.is_null() || out_info.is_null() {
        return;
    }

    let info_handle = unsafe { &*info_handle };
    let info = &info_handle.info;

    unsafe {
        (*out_info).duration = info.duration;
        (*out_info).container_format = info_handle.container_format_cstr.as_ptr();
        (*out_info).video_track_count = info.video_tracks.len() as i32;
        (*out_info).audio_track_count = info.audio_tracks.len() as i32;
    }
}

/// Get video track info
#[no_mangle]
pub extern "C" fn cyb_media_info_get_video_track(
    info_handle: *const CybMediaInfoHandle,
    index: i32,
    out_track: *mut CybVideoTrack,
) -> CybResult {
    if info_handle.is_null() || out_track.is_null() {
        return CybResult::ErrorInvalidHandle;
    }

    let info_handle = unsafe { &*info_handle };
    let info = &info_handle.info;

    if index < 0 || index as usize >= info.video_tracks.len() {
        set_last_error("Video track index out of bounds");
        return CybResult::ErrorUnknown;
    }

    let track = &info.video_tracks[index as usize];

    let zero_rate = CybFrameRate { num: 0, den: 0 };
    let zero_tc = CybTimecode {
        frame_number: 0,
        rate: zero_rate,
        drop_frame: false,
        source_kind: CybTimecodeSourceKind::Unknown,
        source_detail: ptr::null(),
        confidence: 0.0,
    };

    let (has_frame_rate_exact, frame_rate_exact) = match track.frame_rate_exact {
        Some(fr) => (true, frame_rate_to_c(fr)),
        None => (false, zero_rate),
    };

    let detail_ptr = info_handle.timecode_source_details[index as usize].as_ptr();
    let (has_start_timecode, start_timecode) = match track.start_timecode.as_ref() {
        Some(tc) => (true, timecode_to_c(tc, detail_ptr)),
        None => (false, zero_tc),
    };

    unsafe {
        (*out_track).index = track.index;
        (*out_track).codec_name = info_handle.codec_names[index as usize].as_ptr();
        (*out_track).codec_long_name = info_handle.codec_long_names[index as usize].as_ptr();
        (*out_track).width = track.width;
        (*out_track).height = track.height;
        (*out_track).frame_rate = track.frame_rate;
        (*out_track).bit_rate = track.bit_rate;
        (*out_track).is_hardware_decodable = track.is_hardware_decodable;
        (*out_track).has_frame_rate_exact = has_frame_rate_exact;
        (*out_track).frame_rate_exact = frame_rate_exact;
        (*out_track).has_start_timecode = has_start_timecode;
        (*out_track).start_timecode = start_timecode;
    }

    CybResult::Success
}

fn frame_rate_to_c(fr: FrameRate) -> CybFrameRate {
    CybFrameRate {
        num: fr.num,
        den: fr.den,
    }
}

fn source_kind_to_c(kind: TimecodeSourceKind) -> CybTimecodeSourceKind {
    match kind {
        TimecodeSourceKind::TmcdTrack => CybTimecodeSourceKind::TmcdTrack,
        TimecodeSourceKind::MxfMaterialPackage => CybTimecodeSourceKind::MxfMaterialPackage,
        TimecodeSourceKind::MxfSourcePackage => CybTimecodeSourceKind::MxfSourcePackage,
        TimecodeSourceKind::ContainerMetadata => CybTimecodeSourceKind::ContainerMetadata,
        TimecodeSourceKind::Inferred => CybTimecodeSourceKind::Inferred,
        TimecodeSourceKind::Unknown => CybTimecodeSourceKind::Unknown,
    }
}

fn timecode_to_c(tc: &Timecode, source_detail: *const c_char) -> CybTimecode {
    CybTimecode {
        frame_number: tc.frame_number,
        rate: frame_rate_to_c(tc.rate),
        drop_frame: tc.drop_frame,
        source_kind: source_kind_to_c(tc.source_kind),
        source_detail,
        confidence: tc.confidence,
    }
}

/// Get audio track info
#[no_mangle]
pub extern "C" fn cyb_media_info_get_audio_track(
    info_handle: *const CybMediaInfoHandle,
    index: i32,
    out_track: *mut CybAudioTrack,
) -> CybResult {
    if info_handle.is_null() || out_track.is_null() {
        return CybResult::ErrorInvalidHandle;
    }

    let info_handle = unsafe { &*info_handle };
    let info = &info_handle.info;

    if index < 0 || index as usize >= info.audio_tracks.len() {
        set_last_error("Audio track index out of bounds");
        return CybResult::ErrorUnknown;
    }

    let track = &info.audio_tracks[index as usize];
    let offset = info.video_tracks.len();

    unsafe {
        (*out_track).index = track.index;
        (*out_track).codec_name = info_handle.codec_names[offset + index as usize].as_ptr();
        (*out_track).codec_long_name = info_handle.codec_long_names[offset + index as usize].as_ptr();
        (*out_track).sample_rate = track.sample_rate;
        (*out_track).channels = track.channels;
        (*out_track).bit_rate = track.bit_rate;
    }

    CybResult::Success
}

/// Release media info handle
#[no_mangle]
pub extern "C" fn cyb_media_info_release(info_handle: *mut CybMediaInfoHandle) {
    if !info_handle.is_null() {
        unsafe {
            drop(Box::from_raw(info_handle));
        }
    }
}

// =============================================================================
// Decoding State
// =============================================================================

/// Check if decoding is active
#[no_mangle]
pub extern "C" fn cyb_decoder_is_decoding(handle: *const CybDecoderHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().is_decoding()
}

// =============================================================================
// Audio Frame Types
// =============================================================================

/// Audio frame data for FFI
#[repr(C)]
pub struct CybAudioFrame {
    /// Raw sample data pointer (interleaved float32)
    pub data: *const f32,
    /// Number of samples per channel
    pub sample_count: u32,
    /// Number of audio channels
    pub channels: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Presentation timestamp in microseconds
    pub pts_us: i64,
    /// Duration in microseconds
    pub duration_us: i64,
    /// Sequential frame number
    pub frame_number: i64,
}

/// Opaque audio frame handle (owns the data)
pub struct CybAudioFrameHandle {
    frame: AudioFrame,
}

/// Get next audio frame in sequence
#[no_mangle]
pub extern "C" fn cyb_decoder_get_next_audio_frame(
    handle: *mut CybDecoderHandle,
    out_frame: *mut *mut CybAudioFrameHandle,
) -> CybResult {
    if handle.is_null() || out_frame.is_null() {
        return CybResult::ErrorInvalidHandle;
    }

    let handle = unsafe { &*handle };
    let mut decoder = handle.decoder.lock();

    match decoder.get_next_audio_frame() {
        Ok(Some(frame)) => {
            let frame_handle = Box::new(CybAudioFrameHandle { frame });
            unsafe {
                *out_frame = Box::into_raw(frame_handle);
            }
            CybResult::Success
        }
        Ok(None) => {
            unsafe {
                *out_frame = ptr::null_mut();
            }
            CybResult::Success
        }
        Err(e) => e.into(),
    }
}

/// Get audio frame data from handle
#[no_mangle]
pub extern "C" fn cyb_audio_frame_get_data(
    frame_handle: *const CybAudioFrameHandle,
    out_frame: *mut CybAudioFrame,
) {
    if frame_handle.is_null() || out_frame.is_null() {
        return;
    }

    let frame_handle = unsafe { &*frame_handle };
    let frame = &frame_handle.frame;

    unsafe {
        (*out_frame).data = frame.data_ptr();
        (*out_frame).sample_count = frame.sample_count;
        (*out_frame).channels = frame.channels;
        (*out_frame).sample_rate = frame.sample_rate;
        (*out_frame).pts_us = frame.pts_us;
        (*out_frame).duration_us = frame.duration_us;
        (*out_frame).frame_number = frame.frame_number;
    }
}

/// Release audio frame handle
#[no_mangle]
pub extern "C" fn cyb_audio_frame_release(frame_handle: *mut CybAudioFrameHandle) {
    if !frame_handle.is_null() {
        unsafe {
            drop(Box::from_raw(frame_handle));
        }
    }
}

/// Check if decoder has audio
#[no_mangle]
pub extern "C" fn cyb_decoder_has_audio(handle: *const CybDecoderHandle) -> bool {
    if handle.is_null() {
        return false;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().has_audio()
}

/// Get audio sample rate
#[no_mangle]
pub extern "C" fn cyb_decoder_get_audio_sample_rate(handle: *const CybDecoderHandle) -> u32 {
    if handle.is_null() {
        return 0;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().audio_sample_rate()
}

/// Get audio channel count
#[no_mangle]
pub extern "C" fn cyb_decoder_get_audio_channels(handle: *const CybDecoderHandle) -> u32 {
    if handle.is_null() {
        return 0;
    }
    let handle = unsafe { &*handle };
    handle.decoder.lock().audio_channels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_handle() {
        assert_eq!(cyb_decoder_prepare(ptr::null_mut()), CybResult::ErrorInvalidHandle);
        assert_eq!(cyb_decoder_is_prepared(ptr::null()), false);
    }

    #[test]
    fn test_version() {
        let version = cyb_get_version();
        assert!(!version.is_null());
    }
}
