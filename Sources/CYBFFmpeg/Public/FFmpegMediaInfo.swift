// FFmpegMediaInfo.swift
// CYBFFmpeg
//
// Complete media information extracted by FFmpeg.
// This is CYBFFmpeg's native output format, independent of CYBMediaHolder.

import Foundation

// MARK: - FFmpegMediaInfo

/// Complete media information extracted by FFmpeg
public struct FFmpegMediaInfo: Sendable {
    /// Source media URL
    public let url: URL

    /// Duration in seconds
    public let duration: Double

    /// Container format (e.g., "matroska", "mp4", "webm")
    public let containerFormat: String

    /// Video tracks in the media
    public let videoTracks: [FFmpegVideoTrack]

    /// Audio tracks in the media
    public let audioTracks: [FFmpegAudioTrack]

    /// Container-level metadata
    public let metadata: [String: String]

    // MARK: Convenience Properties

    /// Whether the media has video content
    public var hasVideo: Bool {
        !videoTracks.isEmpty
    }

    /// Whether the media has audio content
    public var hasAudio: Bool {
        !audioTracks.isEmpty
    }

    /// Primary video track (first track)
    public var primaryVideoTrack: FFmpegVideoTrack? {
        videoTracks.first
    }

    /// Primary audio track (first track)
    public var primaryAudioTrack: FFmpegAudioTrack? {
        audioTracks.first
    }

    /// Video dimensions of primary track
    public var videoSize: CGSize? {
        guard let track = primaryVideoTrack else { return nil }
        return CGSize(width: track.width, height: track.height)
    }

    /// Frame rate of primary video track
    public var frameRate: Double? {
        primaryVideoTrack?.frameRate
    }

    /// Whether any video track supports hardware decoding
    public var hasHardwareDecodableTrack: Bool {
        videoTracks.contains { $0.isHardwareDecodable }
    }
}

// MARK: - FFmpegVideoTrack

/// Video track information
public struct FFmpegVideoTrack: Sendable {
    /// Track index in container
    public let index: Int

    /// Codec information
    public let codec: FFmpegCodec

    /// Video width in pixels
    public let width: Int

    /// Video height in pixels
    public let height: Int

    /// Frame rate (frames per second)
    public let frameRate: Double

    /// Bit rate in bits per second (if available)
    public let bitRate: Int64?

    /// FFmpeg pixel format string (e.g., "yuv420p", "yuv420p10le")
    public let pixelFormat: String

    /// Whether VideoToolbox hardware decoding is available
    public let isHardwareDecodable: Bool

    /// Color space (e.g., "bt709", "bt2020nc")
    public let colorSpace: String?

    /// Color primaries (e.g., "bt709", "bt2020")
    public let colorPrimaries: String?

    /// Color transfer function (e.g., "bt709", "smpte2084")
    public let colorTransfer: String?

    /// Color range
    public let colorRange: ColorRange

    // MARK: Convenience Properties

    /// Video dimensions as CGSize
    public var size: CGSize {
        CGSize(width: width, height: height)
    }

    /// Aspect ratio (width / height)
    public var aspectRatio: Double {
        guard height > 0 else { return 0 }
        return Double(width) / Double(height)
    }

    /// Whether this is HDR content
    public var isHDR: Bool {
        colorTransfer == "smpte2084" || colorTransfer == "arib-std-b67"
    }

    /// Whether this is 10-bit or higher content
    public var isHighBitDepth: Bool {
        pixelFormat.contains("10") || pixelFormat.contains("12") || pixelFormat.contains("16")
    }
}

// MARK: - FFmpegAudioTrack

/// Audio track information
public struct FFmpegAudioTrack: Sendable {
    /// Track index in container
    public let index: Int

    /// Codec information
    public let codec: FFmpegCodec

    /// Sample rate in Hz
    public let sampleRate: Int

    /// Number of audio channels
    public let channels: Int

    /// Channel layout string (e.g., "stereo", "5.1", "7.1")
    public let channelLayout: String?

    /// Bit rate in bits per second (if available)
    public let bitRate: Int64?

    /// ISO 639 language code (if available)
    public let languageCode: String?

    // MARK: Convenience Properties

    /// Whether this is surround sound (more than 2 channels)
    public var isSurround: Bool {
        channels > 2
    }

    /// Whether this is stereo audio
    public var isStereo: Bool {
        channels == 2 || channelLayout == "stereo"
    }

    /// Whether this is mono audio
    public var isMono: Bool {
        channels == 1 || channelLayout == "mono"
    }
}

// MARK: - FFmpegCodec

/// Codec identification
public struct FFmpegCodec: Sendable {
    /// Short codec name (e.g., "vp9", "av1", "h264")
    public let name: String

    /// Full codec name (e.g., "Google VP9", "AV1 (AOMedia)")
    public let longName: String

    /// FourCC code if available (e.g., "vp09", "av01", "avc1")
    public let fourCC: String?

    /// Known codec categories
    public var category: CodecCategory {
        switch name.lowercased() {
        case "h264", "avc", "avc1":
            return .h264
        case "hevc", "h265", "hvc1", "hev1":
            return .hevc
        case "vp8":
            return .vp8
        case "vp9":
            return .vp9
        case "av1", "libaom-av1", "libdav1d":
            return .av1
        case "prores", "prores_ks":
            return .prores
        case "dnxhd", "dnxhr":
            return .dnxhd
        case "mpeg1video":
            return .mpeg1
        case "mpeg2video":
            return .mpeg2
        case "mpeg4":
            return .mpeg4
        case "mjpeg":
            return .mjpeg
        default:
            return .other
        }
    }

    /// Whether this codec typically has all keyframes (intra-only)
    public var isIntraOnly: Bool {
        switch category {
        case .prores, .dnxhd, .mjpeg:
            return true
        default:
            return false
        }
    }
}

// MARK: - Supporting Types

/// Color range enumeration
public enum ColorRange: String, Sendable, Codable {
    case full = "full"
    case limited = "limited"
    case unknown = "unknown"
}

/// Codec category for quick identification
public enum CodecCategory: String, Sendable {
    case h264
    case hevc
    case vp8
    case vp9
    case av1
    case prores
    case dnxhd
    case mpeg1
    case mpeg2
    case mpeg4
    case mjpeg
    case other
}

// MARK: - Codable Conformance

extension FFmpegMediaInfo: Codable {}
extension FFmpegVideoTrack: Codable {}
extension FFmpegAudioTrack: Codable {}
extension FFmpegCodec: Codable {}
extension CodecCategory: Codable {}

// MARK: - Equatable Conformance

extension FFmpegMediaInfo: Equatable {
    public static func == (lhs: FFmpegMediaInfo, rhs: FFmpegMediaInfo) -> Bool {
        lhs.url == rhs.url &&
        lhs.duration == rhs.duration &&
        lhs.containerFormat == rhs.containerFormat
    }
}

extension FFmpegVideoTrack: Equatable {}
extension FFmpegAudioTrack: Equatable {}
extension FFmpegCodec: Equatable {}

// MARK: - CustomStringConvertible

extension FFmpegMediaInfo: CustomStringConvertible {
    public var description: String {
        """
        FFmpegMediaInfo(
            url: \(url.lastPathComponent),
            duration: \(String(format: "%.2f", duration))s,
            format: \(containerFormat),
            video: \(videoTracks.count) track(s),
            audio: \(audioTracks.count) track(s)
        )
        """
    }
}

extension FFmpegVideoTrack: CustomStringConvertible {
    public var description: String {
        "\(codec.name) \(width)x\(height) @ \(String(format: "%.2f", frameRate))fps"
    }
}

extension FFmpegAudioTrack: CustomStringConvertible {
    public var description: String {
        "\(codec.name) \(sampleRate)Hz \(channelLayout ?? "\(channels)ch")"
    }
}
