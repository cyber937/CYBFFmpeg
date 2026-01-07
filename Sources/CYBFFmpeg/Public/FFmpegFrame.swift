// FFmpegFrame.swift
// CYBFFmpeg
//
// Decoded video frame data with CVPixelBuffer for Metal rendering.

import Foundation
import CoreVideo
import CoreMedia

// MARK: - FFmpegFrame

/// Decoded video frame containing pixel data and timing information
public struct FFmpegFrame: @unchecked Sendable {
    /// Decoded frame as CVPixelBuffer (suitable for Metal rendering)
    public let pixelBuffer: CVPixelBuffer

    /// Presentation timestamp in seconds
    public let presentationTime: Double

    /// Frame duration in seconds
    public let duration: Double

    /// Whether this frame is a keyframe (I-frame)
    public let isKeyframe: Bool

    /// Frame width in pixels
    public let width: Int

    /// Frame height in pixels
    public let height: Int

    /// Sequential frame number from start of media
    public let frameNumber: Int64

    // MARK: Initialization

    /// Internal initializer for creating frames from decoded data
    internal init(
        pixelBuffer: CVPixelBuffer,
        presentationTime: Double,
        duration: Double,
        isKeyframe: Bool,
        width: Int,
        height: Int,
        frameNumber: Int64
    ) {
        self.pixelBuffer = pixelBuffer
        self.presentationTime = presentationTime
        self.duration = duration
        self.isKeyframe = isKeyframe
        self.width = width
        self.height = height
        self.frameNumber = frameNumber
    }

    // MARK: Convenience Properties

    /// Frame dimensions as CGSize
    public var size: CGSize {
        CGSize(width: width, height: height)
    }

    /// Presentation time as CMTime (for AVFoundation interop)
    public var cmTime: CMTime {
        CMTime(seconds: presentationTime, preferredTimescale: 600)
    }

    /// Frame duration as CMTime
    public var cmDuration: CMTime {
        CMTime(seconds: duration, preferredTimescale: 600)
    }

    /// Pixel buffer format type
    public var pixelFormatType: OSType {
        CVPixelBufferGetPixelFormatType(pixelBuffer)
    }

    /// Pixel buffer bytes per row
    public var bytesPerRow: Int {
        CVPixelBufferGetBytesPerRow(pixelBuffer)
    }

    /// Whether the pixel buffer is planar
    public var isPlanar: Bool {
        CVPixelBufferIsPlanar(pixelBuffer)
    }

    /// Number of planes in planar format
    public var planeCount: Int {
        CVPixelBufferGetPlaneCount(pixelBuffer)
    }

    /// Human-readable pixel format description
    public var pixelFormatDescription: String {
        switch pixelFormatType {
        case kCVPixelFormatType_32BGRA:
            return "BGRA"
        case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
            return "NV12 (Video Range)"
        case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
            return "NV12 (Full Range)"
        case kCVPixelFormatType_420YpCbCr8Planar:
            return "YUV420P"
        case kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange:
            return "P010 (Video Range)"
        case kCVPixelFormatType_420YpCbCr10BiPlanarFullRange:
            return "P010 (Full Range)"
        default:
            return "Unknown (\(pixelFormatType))"
        }
    }
}

// MARK: - CustomStringConvertible

extension FFmpegFrame: CustomStringConvertible {
    public var description: String {
        let keyframeMarker = isKeyframe ? " [K]" : ""
        return "Frame #\(frameNumber)\(keyframeMarker) @ \(String(format: "%.3f", presentationTime))s (\(width)x\(height) \(pixelFormatDescription))"
    }
}

// MARK: - CustomDebugStringConvertible

extension FFmpegFrame: CustomDebugStringConvertible {
    public var debugDescription: String {
        """
        FFmpegFrame(
            frameNumber: \(frameNumber),
            presentationTime: \(presentationTime),
            duration: \(duration),
            isKeyframe: \(isKeyframe),
            size: \(width)x\(height),
            format: \(pixelFormatDescription),
            bytesPerRow: \(bytesPerRow),
            planar: \(isPlanar),
            planes: \(planeCount)
        )
        """
    }
}

// MARK: - Equatable (by identity)

extension FFmpegFrame: Equatable {
    public static func == (lhs: FFmpegFrame, rhs: FFmpegFrame) -> Bool {
        lhs.frameNumber == rhs.frameNumber &&
        lhs.presentationTime == rhs.presentationTime
    }
}

// MARK: - Hashable

extension FFmpegFrame: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(frameNumber)
        hasher.combine(presentationTime)
    }
}
