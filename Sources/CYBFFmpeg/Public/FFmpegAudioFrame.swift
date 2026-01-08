// FFmpegAudioFrame.swift
// CYBFFmpeg
//
// Audio frame data decoded by FFmpeg.

import Foundation

// MARK: - FFmpegAudioFrame

/// Decoded audio frame from FFmpeg
public struct FFmpegAudioFrame: Sendable {
    /// Interleaved audio sample data (Float32)
    /// Format: [L0, R0, L1, R1, ...] for stereo
    public let samples: [Float]

    /// Number of samples per channel
    public let sampleCount: Int

    /// Number of audio channels
    public let channels: Int

    /// Sample rate in Hz
    public let sampleRate: Int

    /// Presentation timestamp in seconds
    public let presentationTime: Double

    /// Duration in seconds
    public let duration: Double

    /// Sequential frame number
    public let frameNumber: Int64

    // MARK: - Convenience Properties

    /// Total number of samples (sampleCount * channels)
    public var totalSamples: Int {
        samples.count
    }

    /// Duration calculated from sample count
    public var calculatedDuration: Double {
        guard sampleRate > 0 else { return 0 }
        return Double(sampleCount) / Double(sampleRate)
    }

    /// Whether this is a stereo frame
    public var isStereo: Bool {
        channels == 2
    }

    /// Whether this is a mono frame
    public var isMono: Bool {
        channels == 1
    }

    // MARK: - Initialization

    /// Create an audio frame
    public init(
        samples: [Float],
        sampleCount: Int,
        channels: Int,
        sampleRate: Int,
        presentationTime: Double,
        duration: Double,
        frameNumber: Int64
    ) {
        self.samples = samples
        self.sampleCount = sampleCount
        self.channels = channels
        self.sampleRate = sampleRate
        self.presentationTime = presentationTime
        self.duration = duration
        self.frameNumber = frameNumber
    }

    // MARK: - Sample Access

    /// Get left channel samples (for stereo audio)
    public func leftChannelSamples() -> [Float] {
        guard channels >= 2 else { return samples }
        return stride(from: 0, to: samples.count, by: channels).map { samples[$0] }
    }

    /// Get right channel samples (for stereo audio)
    public func rightChannelSamples() -> [Float] {
        guard channels >= 2 else { return samples }
        return stride(from: 1, to: samples.count, by: channels).map { samples[$0] }
    }

    /// Get samples for a specific channel
    /// - Parameter channel: Channel index (0 for left, 1 for right, etc.)
    /// - Returns: Array of samples for the specified channel
    public func samplesForChannel(_ channel: Int) -> [Float] {
        guard channel < channels else { return [] }
        return stride(from: channel, to: samples.count, by: channels).map { samples[$0] }
    }

    /// Calculate RMS (Root Mean Square) amplitude
    public func rmsAmplitude() -> Float {
        guard !samples.isEmpty else { return 0 }
        let sumOfSquares = samples.reduce(Float(0)) { $0 + $1 * $1 }
        return sqrt(sumOfSquares / Float(samples.count))
    }

    /// Calculate peak amplitude
    public func peakAmplitude() -> Float {
        samples.reduce(Float(0)) { max(abs($0), abs($1)) }
    }

    /// Calculate RMS for each channel
    public func rmsPerChannel() -> [Float] {
        (0..<channels).map { channel in
            let channelSamples = samplesForChannel(channel)
            guard !channelSamples.isEmpty else { return 0 }
            let sumOfSquares = channelSamples.reduce(Float(0)) { $0 + $1 * $1 }
            return sqrt(sumOfSquares / Float(channelSamples.count))
        }
    }
}

// MARK: - CustomStringConvertible

extension FFmpegAudioFrame: CustomStringConvertible {
    public var description: String {
        "FFmpegAudioFrame(\(sampleCount) samples, \(channels)ch, \(sampleRate)Hz, pts: \(String(format: "%.3f", presentationTime))s)"
    }
}

// MARK: - Equatable

extension FFmpegAudioFrame: Equatable {
    public static func == (lhs: FFmpegAudioFrame, rhs: FFmpegAudioFrame) -> Bool {
        lhs.frameNumber == rhs.frameNumber &&
        lhs.presentationTime == rhs.presentationTime &&
        lhs.sampleCount == rhs.sampleCount &&
        lhs.channels == rhs.channels &&
        lhs.sampleRate == rhs.sampleRate
    }
}

// MARK: - Hashable

extension FFmpegAudioFrame: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(frameNumber)
        hasher.combine(presentationTime)
    }
}
