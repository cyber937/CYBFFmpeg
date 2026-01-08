// RustBridge.swift
// CYBFFmpeg
//
// Internal bridge layer between Swift and Rust FFI.

import Foundation
import CoreVideo
import CybFFmpegC

// MARK: - RustBridge

/// Internal class that bridges Swift to the Rust FFI layer
internal final class RustBridge: @unchecked Sendable {
    // MARK: - Static Initialization

    /// Initialize the Rust library once at first use
    private static let initOnce: Void = {
        cyb_init()
    }()

    // MARK: - Properties

    private var handle: OpaquePointer?
    private let lock = NSLock()
    private var isDestroyed = false

    /// Current playhead time in seconds
    var currentTime: Double {
        guard let handle = handle else { return 0 }
        let micros = cyb_decoder_get_current_time(handle)
        return Double(micros) / 1_000_000.0
    }

    // MARK: - Initialization

    /// Create a new Rust bridge
    init(path: String, configuration: DecoderConfiguration) throws {
        // Ensure Rust library is initialized
        _ = Self.initOnce

        var config = configuration.toCybConfig()

        guard let handle = cyb_decoder_create(path, &config) else {
            let error = Self.getLastError()
            throw FFmpegError.invalidFormat(error ?? "Failed to create decoder")
        }

        self.handle = handle
    }

    deinit {
        destroy()
    }

    // MARK: - Lifecycle

    /// Prepare the decoder
    func prepare() throws {
        try withHandle { handle in
            let result = cyb_decoder_prepare(handle)
            try Self.checkResult(result)
        }
    }

    /// Destroy the decoder
    func destroy() {
        lock.withLock {
            guard !isDestroyed, let handle = handle else { return }
            cyb_decoder_destroy(handle)
            self.handle = nil
            isDestroyed = true
        }
    }

    /// Check if prepared
    func isPrepared() -> Bool {
        guard let handle = handle else { return false }
        return cyb_decoder_is_prepared(handle)
    }

    // MARK: - Media Information

    /// Get media information
    func getMediaInfo(url: URL) throws -> FFmpegMediaInfo {
        try withHandle { handle in
            var infoHandle: OpaquePointer?
            let result = cyb_decoder_get_media_info(handle, &infoHandle)
            try Self.checkResult(result)

            guard let infoHandle = infoHandle else {
                throw FFmpegError.notPrepared
            }

            defer { cyb_media_info_release(infoHandle) }

            return Self.convertMediaInfo(infoHandle, url: url)
        }
    }

    // MARK: - Decoding Control

    /// Start sequential decoding
    func startDecoding() {
        guard let handle = handle else { return }
        _ = cyb_decoder_start(handle)
    }

    /// Stop sequential decoding
    func stopDecoding() {
        guard let handle = handle else { return }
        _ = cyb_decoder_stop(handle)
    }

    /// Check if decoding
    func isDecoding() -> Bool {
        guard let handle = handle else { return false }
        return cyb_decoder_is_decoding(handle)
    }

    // MARK: - Frame Access

    /// Get frame at specific time
    func getFrame(at time: Double, tolerance: Double) throws -> FFmpegFrame? {
        try withHandle { handle in
            let timeMicros = Int64(time * 1_000_000)
            let toleranceMicros = Int64(tolerance * 1_000_000)

            var frameHandle: OpaquePointer?
            let result = cyb_decoder_get_frame_at(handle, timeMicros, toleranceMicros, &frameHandle)
            try Self.checkResult(result)

            guard let frameHandle = frameHandle else {
                return nil
            }

            defer { cyb_frame_release(frameHandle) }
            return try Self.convertFrame(frameHandle)
        }
    }

    /// Get next frame
    func getNextFrame() -> FFmpegFrame? {
        guard let handle = handle else { return nil }

        var frameHandle: OpaquePointer?
        let result = cyb_decoder_get_next_frame(handle, &frameHandle)

        guard result == CYB_RESULT_SUCCESS, let frameHandle = frameHandle else {
            return nil
        }

        defer { cyb_frame_release(frameHandle) }
        return try? Self.convertFrame(frameHandle)
    }

    /// Seek to time (frame-accurate seek)
    /// This performs a keyframe seek first, then decodes frames until reaching the target time.
    /// Returns the frame at or just before the target time.
    func seek(to time: Double) throws -> FFmpegFrame? {
        try withHandle { handle in
            let timeMicros = Int64(time * 1_000_000)

            var frameHandle: OpaquePointer?
            let result = cyb_decoder_seek_precise(handle, timeMicros, &frameHandle)
            try Self.checkResult(result)

            guard let frameHandle = frameHandle else {
                return nil
            }

            defer { cyb_frame_release(frameHandle) }
            return try Self.convertFrame(frameHandle)
        }
    }

    /// Seek to keyframe (fast but not frame-accurate)
    /// This only seeks to the nearest keyframe without decoding intermediate frames.
    func seekToKeyframe(at time: Double) throws {
        try withHandle { handle in
            let timeMicros = Int64(time * 1_000_000)
            let result = cyb_decoder_seek(handle, timeMicros)
            try Self.checkResult(result)
        }
    }

    // MARK: - Prefetch

    /// Start prefetch
    func startPrefetch(direction: Int32, velocity: Double) {
        guard let handle = handle else { return }
        _ = cyb_decoder_start_prefetch(handle, direction, velocity)
    }

    /// Stop prefetch
    func stopPrefetch() {
        guard let handle = handle else { return }
        _ = cyb_decoder_stop_prefetch(handle)
    }

    /// Check if prefetching
    func isPrefetching() -> Bool {
        guard let handle = handle else { return false }
        return cyb_decoder_is_prefetching(handle)
    }

    // MARK: - Cache

    /// Get cache statistics
    func getCacheStatistics() -> CacheStatistics {
        guard let handle = handle else { return .empty }

        var cybStats = CybCacheStats()
        cyb_decoder_get_cache_stats(handle, &cybStats)

        return CacheStatistics(
            l1Entries: Int(cybStats.l1_entries),
            l2Entries: Int(cybStats.l2_entries),
            l3Entries: Int(cybStats.l3_entries),
            l1HitCount: Int(cybStats.l1_hit_count),
            l2HitCount: Int(cybStats.l2_hit_count),
            l3HitCount: Int(cybStats.l3_hit_count),
            missCount: Int(cybStats.miss_count),
            memoryUsageBytes: Int(cybStats.memory_usage_bytes)
        )
    }

    /// Clear cache
    func clearCache() throws {
        try withHandle { handle in
            let result = cyb_decoder_clear_cache(handle)
            try Self.checkResult(result)
        }
    }

    // MARK: - Audio

    /// Check if decoder has audio
    func hasAudio() -> Bool {
        guard let handle = handle else { return false }
        return cyb_decoder_has_audio(handle)
    }

    /// Get audio sample rate
    func audioSampleRate() -> Int {
        guard let handle = handle else { return 0 }
        return Int(cyb_decoder_get_audio_sample_rate(handle))
    }

    /// Get audio channel count
    func audioChannels() -> Int {
        guard let handle = handle else { return 0 }
        return Int(cyb_decoder_get_audio_channels(handle))
    }

    /// Get next audio frame
    func getNextAudioFrame() -> FFmpegAudioFrame? {
        guard let handle = handle else { return nil }

        var frameHandle: OpaquePointer?
        let result = cyb_decoder_get_next_audio_frame(handle, &frameHandle)

        guard result == CYB_RESULT_SUCCESS, let frameHandle = frameHandle else {
            return nil
        }

        defer { cyb_audio_frame_release(frameHandle) }
        return Self.convertAudioFrame(frameHandle)
    }

    /// Prime audio decoder after seek.
    /// Call this after seek() and before getNextAudioFrame() to ensure
    /// audio packets are pre-loaded into the queue for immediate decoding.
    /// This is necessary because after seek, the first packets read from the
    /// stream may be video packets, leaving the audio queue empty.
    /// Returns the number of audio packets that were queued.
    func primeAudioAfterSeek() -> Int {
        guard let handle = handle else { return 0 }
        return Int(cyb_decoder_prime_audio_after_seek(handle))
    }

    // MARK: - Private Helpers

    private func withHandle<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        lock.lock()
        defer { lock.unlock() }

        guard !isDestroyed, let handle = handle else {
            throw FFmpegError.invalidHandle
        }

        return try body(handle)
    }

    // MARK: - Static Helpers

    private static func getLastError() -> String? {
        guard let cStr = cyb_get_last_error() else { return nil }
        return String(cString: cStr)
    }

    private static func checkResult(_ result: CybResult) throws {
        guard result != CYB_RESULT_SUCCESS else { return }

        let errorMessage = getLastError() ?? "Unknown error"

        switch result {
        case CYB_RESULT_ERROR_FILE_NOT_FOUND:
            throw FFmpegError.fileNotFound(URL(fileURLWithPath: errorMessage))
        case CYB_RESULT_ERROR_INVALID_FORMAT:
            throw FFmpegError.invalidFormat(errorMessage)
        case CYB_RESULT_ERROR_CODEC_NOT_SUPPORTED:
            throw FFmpegError.codecNotSupported(errorMessage)
        case CYB_RESULT_ERROR_DECODE_FAILED:
            throw FFmpegError.decodeFailed(errorMessage)
        case CYB_RESULT_ERROR_SEEK_FAILED:
            throw FFmpegError.seekFailed(0)
        case CYB_RESULT_ERROR_MEMORY:
            throw FFmpegError.memoryError
        case CYB_RESULT_ERROR_INVALID_HANDLE:
            throw FFmpegError.invalidHandle
        case CYB_RESULT_ERROR_NOT_PREPARED:
            throw FFmpegError.notPrepared
        default:
            throw FFmpegError.unknown(Int32(result.rawValue))
        }
    }

    // MARK: - Type Conversion

    private static func convertMediaInfo(_ infoHandle: OpaquePointer, url: URL) -> FFmpegMediaInfo {
        var cybInfo = CybMediaInfo()
        cyb_media_info_get_details(infoHandle, &cybInfo)

        var videoTracks: [FFmpegVideoTrack] = []
        var audioTracks: [FFmpegAudioTrack] = []

        // Convert video tracks
        for i in 0..<cybInfo.video_track_count {
            var track = CybVideoTrack()
            if cyb_media_info_get_video_track(infoHandle, i, &track) == CYB_RESULT_SUCCESS {
                videoTracks.append(convertVideoTrack(track))
            }
        }

        // Convert audio tracks
        for i in 0..<cybInfo.audio_track_count {
            var track = CybAudioTrack()
            if cyb_media_info_get_audio_track(infoHandle, i, &track) == CYB_RESULT_SUCCESS {
                audioTracks.append(convertAudioTrack(track))
            }
        }

        let containerFormat = cybInfo.container_format.map { String(cString: $0) } ?? "unknown"

        return FFmpegMediaInfo(
            url: url,
            duration: cybInfo.duration,
            containerFormat: containerFormat,
            videoTracks: videoTracks,
            audioTracks: audioTracks,
            metadata: [:]
        )
    }

    private static func convertVideoTrack(_ cyb: CybVideoTrack) -> FFmpegVideoTrack {
        let codecName = cyb.codec_name.map { String(cString: $0) } ?? "unknown"
        let codecLongName = cyb.codec_long_name.map { String(cString: $0) } ?? "Unknown"

        return FFmpegVideoTrack(
            index: Int(cyb.index),
            codec: FFmpegCodec(
                name: codecName,
                longName: codecLongName,
                fourCC: nil
            ),
            width: Int(cyb.width),
            height: Int(cyb.height),
            frameRate: cyb.frame_rate,
            bitRate: cyb.bit_rate > 0 ? cyb.bit_rate : nil,
            pixelFormat: "unknown",
            isHardwareDecodable: cyb.is_hardware_decodable,
            colorSpace: nil,
            colorPrimaries: nil,
            colorTransfer: nil,
            colorRange: .unknown
        )
    }

    private static func convertAudioTrack(_ cyb: CybAudioTrack) -> FFmpegAudioTrack {
        let codecName = cyb.codec_name.map { String(cString: $0) } ?? "unknown"
        let codecLongName = cyb.codec_long_name.map { String(cString: $0) } ?? "Unknown"

        return FFmpegAudioTrack(
            index: Int(cyb.index),
            codec: FFmpegCodec(
                name: codecName,
                longName: codecLongName,
                fourCC: nil
            ),
            sampleRate: Int(cyb.sample_rate),
            channels: Int(cyb.channels),
            channelLayout: nil,
            bitRate: cyb.bit_rate > 0 ? cyb.bit_rate : nil,
            languageCode: nil
        )
    }

    private static func convertFrame(_ frameHandle: OpaquePointer) throws -> FFmpegFrame {
        var cybFrame = CybVideoFrame()
        cyb_frame_get_data(frameHandle, &cybFrame)

        // Create CVPixelBuffer from raw data
        let format = PixelFormat(rawValue: cybFrame.pixel_format)

        let pixelBuffer = try PixelBufferConverter.convert(
            data: cybFrame.data,
            width: Int(cybFrame.width),
            height: Int(cybFrame.height),
            stride: Int(cybFrame.stride),
            format: format,
            dataSize: Int(cybFrame.data_size)
        )

        return FFmpegFrame(
            pixelBuffer: pixelBuffer,
            presentationTime: Double(cybFrame.pts_us) / 1_000_000.0,
            duration: Double(cybFrame.duration_us) / 1_000_000.0,
            isKeyframe: cybFrame.is_keyframe,
            width: Int(cybFrame.width),
            height: Int(cybFrame.height),
            frameNumber: cybFrame.frame_number
        )
    }

    private static func convertAudioFrame(_ frameHandle: OpaquePointer) -> FFmpegAudioFrame {
        var cybFrame = CybAudioFrame()
        cyb_audio_frame_get_data(frameHandle, &cybFrame)

        // Copy audio samples from FFI pointer
        let totalSamples = Int(cybFrame.sample_count) * Int(cybFrame.channels)
        var samples: [Float] = []

        if let dataPtr = cybFrame.data, totalSamples > 0 {
            samples = Array(UnsafeBufferPointer(start: dataPtr, count: totalSamples))
        }

        return FFmpegAudioFrame(
            samples: samples,
            sampleCount: Int(cybFrame.sample_count),
            channels: Int(cybFrame.channels),
            sampleRate: Int(cybFrame.sample_rate),
            presentationTime: Double(cybFrame.pts_us) / 1_000_000.0,
            duration: Double(cybFrame.duration_us) / 1_000_000.0,
            frameNumber: cybFrame.frame_number
        )
    }
}

// MARK: - Configuration Extensions

extension DecoderConfiguration {
    func toCybConfig() -> CybDecoderConfig {
        CybDecoderConfig(
            prefer_hardware_decoding: preferHardwareDecoding,
            cache_config: cacheConfiguration.toCybConfig(),
            thread_count: UInt32(threadCount),
            output_pixel_format: outputPixelFormat.toCybFormat()
        )
    }
}

extension CacheConfiguration {
    func toCybConfig() -> CybCacheConfig {
        CybCacheConfig(
            l1_capacity: UInt32(l1Capacity),
            l2_capacity: UInt32(l2Capacity),
            l3_capacity: UInt32(l3Capacity),
            enable_prefetch: enablePrefetch
        )
    }
}

extension PixelFormat {
    func toCybFormat() -> UInt8 {
        switch self {
        case .bgra:
            return 0
        case .nv12:
            return 1
        case .yuv420p:
            return 2
        }
    }

    init(rawValue: UInt8) {
        switch rawValue {
        case 0:
            self = .bgra
        case 1:
            self = .nv12
        case 2:
            self = .yuv420p
        default:
            self = .bgra
        }
    }
}
