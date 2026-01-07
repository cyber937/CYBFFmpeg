// CYBFFmpeg.swift
// CYBFFmpeg
//
// Main module exports for CYBFFmpeg package.

import Foundation

// MARK: - Version Information

/// CYBFFmpeg version information
public enum CYBFFmpegVersion {
    /// Current version string
    public static let version = "0.1.0"

    /// Build type
    public static let buildType = "development"

    /// Minimum macOS version
    public static let minimumMacOSVersion = "14.0"

    /// FFmpeg version (when available)
    public static var ffmpegVersion: String {
        // Will be populated by Rust bridge when available
        guard let version = cyb_get_ffmpeg_version() else {
            return "Not loaded"
        }
        return String(cString: version)
    }

    /// Full version description
    public static var fullDescription: String {
        """
        CYBFFmpeg \(version) (\(buildType))
        FFmpeg: \(ffmpegVersion)
        macOS: \(minimumMacOSVersion)+
        """
    }
}

// MARK: - Module Re-exports

// Public API types are exported via their respective files:
// - FFmpegMediaInfo.swift: FFmpegMediaInfo, FFmpegVideoTrack, FFmpegAudioTrack, FFmpegCodec
// - FFmpegFrame.swift: FFmpegFrame
// - FFmpegDecoder.swift: FFmpegDecoder, FFmpegFrameProvider, FFmpegDecoderDelegate
// - FFmpegError.swift: FFmpegError
// - Configuration.swift: DecoderConfiguration, CacheConfiguration, PixelFormat, CacheStatistics

// MARK: - Import C FFI

@_exported import CybFFmpegC
