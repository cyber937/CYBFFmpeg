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

// MARK: - FFmpegFrameRate

/// Lossless rational frame rate (e.g., `24000/1001` for 23.976 fps).
///
/// Use this instead of `Double` when SMPTE timecode arithmetic matters —
/// converting an NTSC rate via `Double` discards the `1/24000s` precision
/// needed for frame-accurate calculations.
public struct FFmpegFrameRate: Sendable, Codable, Equatable, Hashable {
    /// Numerator (e.g., `24000`).
    public let numerator: UInt32
    /// Denominator (e.g., `1001`).
    public let denominator: UInt32

    public init(numerator: UInt32, denominator: UInt32) {
        self.numerator = numerator
        self.denominator = denominator
    }

    /// Approximate `Double` representation. Lossy for NTSC rates.
    public var asDouble: Double {
        guard denominator > 0 else { return 0 }
        return Double(numerator) / Double(denominator)
    }

    /// Whether this is an NTSC (drop-frame–capable) rate.
    public var isNTSC: Bool { denominator == 1001 }
}

// MARK: - FFmpegTimecodeSourceKind

/// Where an embedded SMPTE timecode was sourced from.
public enum FFmpegTimecodeSourceKind: UInt8, Sendable, Codable {
    /// AVFoundation timecode track (`tmcd`) — high-confidence.
    case tmcdTrack = 0
    /// MXF MaterialPackage timecode — high-confidence for Sony / Panasonic / etc.
    case mxfMaterialPackage = 1
    /// MXF SourcePackage timecode — original camera TC, separate from material.
    case mxfSourcePackage = 2
    /// Container-level metadata `timecode` tag (MOV/MP4/etc.).
    case containerMetadata = 3
    /// No embedded TC found — value was inferred from frame rate.
    case inferred = 4
    /// Unknown source.
    case unknown = 5
}

// MARK: - FFmpegTimecode

/// Embedded SMPTE timecode read from a media file.
///
/// Canonical representation is the 5-tuple
/// `(frameNumber, rate, dropFrame, sourceKind, confidence)`. Frame
/// number is the absolute integer frame count from `00:00:00:00`,
/// preserving SMPTE 12M drop-frame arithmetic exactly. `displayString`
/// and `seconds` are derived (and lossy in the case of `seconds`).
public struct FFmpegTimecode: Sendable, Codable, Equatable, Hashable {
    /// Absolute frame count from `00:00:00:00`.
    public let frameNumber: Int64
    /// Exact frame rate.
    public let rate: FFmpegFrameRate
    /// True for 29.97 / 59.94 drop-frame timecode.
    public let dropFrame: Bool
    /// Where the timecode was sourced from.
    public let sourceKind: FFmpegTimecodeSourceKind
    /// Free-form provenance (e.g., `"ffmpeg:mxf:format:timecode"`).
    public let sourceDetail: String
    /// Confidence in the value, `0.0...1.0`.
    public let confidence: Float

    public init(
        frameNumber: Int64,
        rate: FFmpegFrameRate,
        dropFrame: Bool,
        sourceKind: FFmpegTimecodeSourceKind,
        sourceDetail: String,
        confidence: Float
    ) {
        self.frameNumber = frameNumber
        self.rate = rate
        self.dropFrame = dropFrame
        self.sourceKind = sourceKind
        self.sourceDetail = sourceDetail
        self.confidence = confidence
    }

    /// Best-effort `HH:MM:SS:FF` (NDF) / `HH:MM:SS;FF` (DF) display string.
    ///
    /// Uses SMPTE 12M label-rate arithmetic (23.976 NDF labels as 24fps,
    /// 29.97 DF as 30fps with the standard frame drops). Always returns
    /// a positive-time string; negative `frameNumber` is clamped to 0.
    public var displayString: String {
        let tcRate = SMPTEArithmetic.tcIntegerRate(rate)
        let (h, m, s, f) = SMPTEArithmetic.framesToComponents(
            max(frameNumber, 0),
            rate: tcRate,
            dropFrame: dropFrame
        )
        let sep = dropFrame ? ";" : ":"
        return String(format: "%02d:%02d:%02d%@%02d", h, m, s, sep, f)
    }

    /// Seconds equivalent. Lossy for NTSC rates — prefer `frameNumber`
    /// for arithmetic.
    public var seconds: Double {
        guard rate.numerator > 0 else { return 0 }
        return Double(frameNumber) * Double(rate.denominator) / Double(rate.numerator)
    }
}

// MARK: - SMPTEArithmetic (internal)

/// Pure-Swift mirror of the Rust `frames_to_smpte_components` /
/// `tc_integer_rate` helpers. Lives in the framework so display
/// strings can be formatted without crossing the FFI boundary.
internal enum SMPTEArithmetic {
    /// SMPTE clock label rate. NTSC family collapses to its integer
    /// rate (23.976 → 24, 29.97 → 30, 59.94 → 60); other rates keep
    /// `num/den`.
    static func tcIntegerRate(_ rate: FFmpegFrameRate) -> UInt32 {
        if rate.denominator == 1001 {
            return rate.numerator / 1000
        }
        if rate.denominator == 0 { return 30 }
        return rate.numerator / rate.denominator
    }

    /// SMPTE 12M frame-number → `(h, m, s, f)`.
    static func framesToComponents(
        _ frames: Int64,
        rate: UInt32,
        dropFrame: Bool
    ) -> (Int, Int, Int, Int) {
        precondition(rate > 0, "rate must be positive")
        let r = Int64(rate)
        let frameNumber: Int64
        if dropFrame {
            // SMPTE 12M DF: drop 2 frames per minute except every 10th, for
            // 29.97 family (rate=30); 4-per-min except every 10th, for 59.94
            // family (rate=60).
            let dropPerMinute: Int64 = (rate == 60) ? 4 : 2
            let framesPer10Min = r * 60 * 10 - 9 * dropPerMinute
            let framesPerMin = r * 60 - dropPerMinute
            let d = frames / framesPer10Min
            let m = frames % framesPer10Min
            if m > dropPerMinute {
                frameNumber = frames
                    + dropPerMinute * 9 * d
                    + dropPerMinute * ((m - dropPerMinute) / framesPerMin)
            } else {
                frameNumber = frames + dropPerMinute * 9 * d
            }
        } else {
            frameNumber = frames
        }

        let f = Int(frameNumber % r)
        let totalSeconds = frameNumber / r
        let s = Int(totalSeconds % 60)
        let totalMinutes = totalSeconds / 60
        let m = Int(totalMinutes % 60)
        let h = Int(totalMinutes / 60)
        return (h, m, s, f)
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

    /// Frame rate (frames per second). Lossy for NTSC family — prefer
    /// `frameRateExact` when present.
    public let frameRate: Double

    /// Lossless rational frame rate (e.g., `24000/1001`). `nil` when
    /// FFmpeg returned an unknown / zero `avg_frame_rate`.
    public let frameRateExact: FFmpegFrameRate?

    /// Embedded SMPTE start timecode if the container exposed one.
    /// `nil` when no `timecode` tag was found at the stream or format
    /// level.
    public let startTimecode: FFmpegTimecode?

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
