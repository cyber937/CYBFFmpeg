// FFmpegDecoder.swift
// CYBFFmpeg
//
// Main decoder class for FFmpeg-based video decoding.

import Foundation
import CoreVideo

// MARK: - FFmpegFrameProvider Protocol

/// Protocol for frame access abstraction
public protocol FFmpegFrameProvider: Sendable {
    /// Media information (available after prepare)
    var mediaInfo: FFmpegMediaInfo? { get }

    /// Current playhead time in seconds
    var currentTime: Double { get }

    /// Get frame at specific time with tolerance
    func getFrame(at time: Double, tolerance: Double) throws -> FFmpegFrame?

    /// Get next frame in sequence
    func getNextFrame() -> FFmpegFrame?

    /// Seek to specific time
    func seek(to time: Double) throws -> FFmpegFrame?

    /// Start prefetching for scrubbing
    func startPrefetch(direction: Int, velocity: Double)

    /// Stop prefetching
    func stopPrefetch()
}

// MARK: - FFmpegDecoderDelegate

/// Delegate for decoder events
public protocol FFmpegDecoderDelegate: AnyObject, Sendable {
    /// Called when a frame is decoded (for continuous decoding)
    func decoder(_ decoder: FFmpegDecoder, didDecodeFrame frame: FFmpegFrame)

    /// Called when an error occurs
    func decoder(_ decoder: FFmpegDecoder, didEncounterError error: FFmpegError)

    /// Called when decode completes (end of media)
    func decoderDidFinishDecoding(_ decoder: FFmpegDecoder)
}

// Default implementations
public extension FFmpegDecoderDelegate {
    func decoder(_ decoder: FFmpegDecoder, didDecodeFrame frame: FFmpegFrame) {}
    func decoder(_ decoder: FFmpegDecoder, didEncounterError error: FFmpegError) {}
    func decoderDidFinishDecoding(_ decoder: FFmpegDecoder) {}
}

// MARK: - FFmpegDecoder

/// Main decoder class for FFmpeg-based video decoding
///
/// Usage:
/// ```swift
/// let decoder = try FFmpegDecoder(url: fileURL)
/// try await decoder.prepare()
///
/// // Get frame at specific time
/// if let frame = try decoder.getFrame(at: 5.0, tolerance: 0.016) {
///     // Use frame.pixelBuffer for rendering
/// }
///
/// // Cleanup
/// decoder.invalidate()
/// ```
public final class FFmpegDecoder: @unchecked Sendable {
    // MARK: - Properties

    /// Source URL
    public let url: URL

    /// Decoder configuration
    public let configuration: DecoderConfiguration

    /// Delegate for events
    public weak var delegate: FFmpegDecoderDelegate?

    /// Media information (available after prepare)
    public private(set) var mediaInfo: FFmpegMediaInfo?

    /// Current playhead time in seconds
    public var currentTime: Double {
        guard let bridge = bridge, !isInvalidated else { return 0 }
        return bridge.currentTime
    }

    /// Cache statistics
    public var cacheStatistics: CacheStatistics {
        guard let bridge = bridge, !isInvalidated else { return .empty }
        return bridge.getCacheStatistics()
    }

    /// Whether the decoder is prepared
    public private(set) var isPrepared: Bool = false

    /// Whether the decoder is currently decoding
    public private(set) var isDecoding: Bool = false

    /// Whether prefetch is active
    public private(set) var isPrefetching: Bool = false

    /// Whether the decoder has been invalidated
    public private(set) var isInvalidated: Bool = false

    // MARK: - Private Properties

    private var bridge: RustBridge?
    private let lock = NSLock()

    // MARK: - Initialization

    /// Create a new decoder
    /// - Parameters:
    ///   - url: URL to the media file
    ///   - configuration: Decoder configuration
    /// - Throws: FFmpegError if initialization fails
    public init(url: URL, configuration: DecoderConfiguration = .default) throws {
        self.url = url
        self.configuration = configuration

        // Verify file exists
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw FFmpegError.fileNotFound(url)
        }

        // Create the Rust bridge (does not load media yet)
        self.bridge = try RustBridge(path: url.path, configuration: configuration)
    }

    deinit {
        invalidate()
    }

    // MARK: - Lifecycle

    /// Prepare the decoder (loads metadata, initializes codecs)
    /// Must be called before any frame access
    public func prepare() async throws {
        try checkNotInvalidated()

        guard !isPrepared else { return }

        guard let bridge = bridge else {
            throw FFmpegError.invalidHandle
        }

        // Prepare on background thread
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    try bridge.prepare()
                    let info = try bridge.getMediaInfo(url: self.url)

                    self.lock.withLock {
                        self.mediaInfo = info
                        self.isPrepared = true
                    }

                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    /// Invalidate the decoder and release all resources
    public func invalidate() {
        lock.withLock {
            guard !isInvalidated else { return }

            isInvalidated = true
            isDecoding = false
            isPrefetching = false
            isPrepared = false

            bridge?.destroy()
            bridge = nil
        }
    }

    // MARK: - Decoding Control

    /// Start sequential decoding (frames delivered via delegate)
    public func startDecoding() {
        lock.withLock {
            guard !isInvalidated, isPrepared, !isDecoding else { return }
            bridge?.startDecoding()
            isDecoding = true
        }
    }

    /// Stop sequential decoding
    public func stopDecoding() {
        lock.withLock {
            guard isDecoding else { return }
            bridge?.stopDecoding()
            isDecoding = false
        }
    }

    // MARK: - Frame Access

    /// Get frame at specific time
    /// - Parameters:
    ///   - time: Target time in seconds
    ///   - tolerance: Acceptable time tolerance in seconds
    /// - Returns: Decoded frame or nil if not found
    /// - Throws: FFmpegError if decoding fails
    public func getFrame(at time: Double, tolerance: Double) throws -> FFmpegFrame? {
        try checkNotInvalidated()
        try checkPrepared()

        guard let bridge = bridge else {
            throw FFmpegError.invalidHandle
        }

        return try bridge.getFrame(at: time, tolerance: tolerance)
    }

    /// Get next frame in sequence (for continuous decoding)
    /// - Returns: Next frame or nil if at end of media
    public func getNextFrame() -> FFmpegFrame? {
        guard !isInvalidated, isPrepared, let bridge = bridge else {
            return nil
        }

        return bridge.getNextFrame()
    }

    /// Seek to specific time
    /// - Parameter time: Target time in seconds
    /// - Returns: Frame at or near the target time
    /// - Throws: FFmpegError if seek fails
    @discardableResult
    public func seek(to time: Double) throws -> FFmpegFrame? {
        try checkNotInvalidated()
        try checkPrepared()

        guard let bridge = bridge else {
            throw FFmpegError.invalidHandle
        }

        return try bridge.seek(to: time)
    }

    // MARK: - Prefetch (Scrubbing)

    /// Start prefetching frames for scrubbing
    /// - Parameters:
    ///   - direction: Navigation direction (1 = forward, -1 = backward)
    ///   - velocity: Speed multiplier (1.0 = normal speed)
    public func startPrefetch(direction: Int, velocity: Double) {
        lock.withLock {
            guard !isInvalidated, isPrepared, let bridge = bridge else { return }

            bridge.startPrefetch(direction: Int32(direction), velocity: velocity)
            isPrefetching = true
        }
    }

    /// Stop prefetching
    public func stopPrefetch() {
        lock.withLock {
            guard isPrefetching, let bridge = bridge else { return }

            bridge.stopPrefetch()
            isPrefetching = false
        }
    }

    // MARK: - Cache Management

    /// Clear all cached frames
    public func clearCache() throws {
        try checkNotInvalidated()

        guard let bridge = bridge else {
            throw FFmpegError.invalidHandle
        }

        try bridge.clearCache()
    }

    // MARK: - Private Helpers

    private func checkNotInvalidated() throws {
        guard !isInvalidated else {
            throw FFmpegError.alreadyInvalidated
        }
    }

    private func checkPrepared() throws {
        guard isPrepared else {
            throw FFmpegError.notPrepared
        }
    }
}

// MARK: - FFmpegFrameProvider Conformance

extension FFmpegDecoder: FFmpegFrameProvider {}

// MARK: - CustomStringConvertible

extension FFmpegDecoder: CustomStringConvertible {
    public var description: String {
        let status: String
        if isInvalidated {
            status = "invalidated"
        } else if !isPrepared {
            status = "not prepared"
        } else if isDecoding {
            status = "decoding"
        } else if isPrefetching {
            status = "prefetching"
        } else {
            status = "ready"
        }

        return "FFmpegDecoder(\(url.lastPathComponent), \(status))"
    }
}
