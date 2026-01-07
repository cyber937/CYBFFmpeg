// FFmpegError.swift
// CYBFFmpeg
//
// Error types for CYBFFmpeg operations.

import Foundation

// MARK: - FFmpegError

/// Errors that can occur during FFmpeg operations
public enum FFmpegError: Error, Sendable {
    /// File not found at the specified URL
    case fileNotFound(URL)

    /// Invalid or unsupported media format
    case invalidFormat(String)

    /// Codec not supported by this build
    case codecNotSupported(String)

    /// Frame decoding failed
    case decodeFailed(String)

    /// Seek operation failed
    case seekFailed(Double)

    /// Memory allocation failed
    case memoryError

    /// Decoder not prepared (prepare() not called)
    case notPrepared

    /// Invalid decoder handle
    case invalidHandle

    /// Decoder already invalidated
    case alreadyInvalidated

    /// Operation cancelled
    case cancelled

    /// Rust panic occurred
    case rustPanic(String)

    /// Unknown error with FFmpeg error code
    case unknown(Int32)
}

// MARK: - LocalizedError

extension FFmpegError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .fileNotFound(let url):
            return "File not found: \(url.lastPathComponent)"
        case .invalidFormat(let format):
            return "Invalid or unsupported format: \(format)"
        case .codecNotSupported(let codec):
            return "Codec not supported: \(codec)"
        case .decodeFailed(let reason):
            return "Decode failed: \(reason)"
        case .seekFailed(let time):
            return "Seek failed at \(String(format: "%.2f", time))s"
        case .memoryError:
            return "Memory allocation failed"
        case .notPrepared:
            return "Decoder not prepared. Call prepare() first."
        case .invalidHandle:
            return "Invalid decoder handle"
        case .alreadyInvalidated:
            return "Decoder has been invalidated"
        case .cancelled:
            return "Operation cancelled"
        case .rustPanic(let message):
            return "Internal error (Rust panic): \(message)"
        case .unknown(let code):
            return "Unknown error (code: \(code))"
        }
    }

    public var failureReason: String? {
        switch self {
        case .fileNotFound:
            return "The specified file does not exist or is not accessible."
        case .invalidFormat:
            return "The media format is not recognized or is corrupted."
        case .codecNotSupported:
            return "This codec is not included in the LGPL build."
        case .decodeFailed:
            return "The decoder encountered an error while processing the frame."
        case .seekFailed:
            return "The requested time position could not be reached."
        case .memoryError:
            return "Insufficient memory to complete the operation."
        case .notPrepared:
            return "The decoder must be prepared before accessing frames."
        case .invalidHandle:
            return "The decoder handle is no longer valid."
        case .alreadyInvalidated:
            return "The decoder has been invalidated and cannot be used."
        case .cancelled:
            return "The operation was cancelled by the user."
        case .rustPanic:
            return "An unexpected error occurred in the native code."
        case .unknown:
            return "An unexpected error occurred."
        }
    }

    public var recoverySuggestion: String? {
        switch self {
        case .fileNotFound:
            return "Check that the file exists and the app has permission to access it."
        case .invalidFormat:
            return "Verify the file is a valid media file."
        case .codecNotSupported:
            return "Try converting the file to a supported format (H.264, VP9, AV1, etc.)."
        case .decodeFailed:
            return "The file may be corrupted. Try re-encoding or using a different file."
        case .seekFailed:
            return "Try seeking to a different time or use a smaller tolerance."
        case .memoryError:
            return "Close other applications to free memory, or reduce cache size."
        case .notPrepared:
            return "Call prepare() on the decoder before accessing frames."
        case .invalidHandle, .alreadyInvalidated:
            return "Create a new decoder instance."
        case .cancelled:
            return nil
        case .rustPanic, .unknown:
            return "Please report this issue with the media file if possible."
        }
    }
}

// MARK: - CustomNSError

extension FFmpegError: CustomNSError {
    public static var errorDomain: String {
        "com.cyberseeds.CYBFFmpeg"
    }

    public var errorCode: Int {
        switch self {
        case .fileNotFound:
            return 1
        case .invalidFormat:
            return 2
        case .codecNotSupported:
            return 3
        case .decodeFailed:
            return 4
        case .seekFailed:
            return 5
        case .memoryError:
            return 6
        case .notPrepared:
            return 7
        case .invalidHandle:
            return 8
        case .alreadyInvalidated:
            return 9
        case .cancelled:
            return 10
        case .rustPanic:
            return 98
        case .unknown(let code):
            return Int(code)
        }
    }

    public var errorUserInfo: [String: Any] {
        var info: [String: Any] = [:]

        if let description = errorDescription {
            info[NSLocalizedDescriptionKey] = description
        }
        if let reason = failureReason {
            info[NSLocalizedFailureReasonErrorKey] = reason
        }
        if let suggestion = recoverySuggestion {
            info[NSLocalizedRecoverySuggestionErrorKey] = suggestion
        }

        // Add specific context
        switch self {
        case .fileNotFound(let url):
            info["url"] = url
        case .seekFailed(let time):
            info["time"] = time
        case .unknown(let code):
            info["ffmpegErrorCode"] = code
        default:
            break
        }

        return info
    }
}

// MARK: - Equatable

extension FFmpegError: Equatable {
    public static func == (lhs: FFmpegError, rhs: FFmpegError) -> Bool {
        switch (lhs, rhs) {
        case (.fileNotFound(let l), .fileNotFound(let r)):
            return l == r
        case (.invalidFormat(let l), .invalidFormat(let r)):
            return l == r
        case (.codecNotSupported(let l), .codecNotSupported(let r)):
            return l == r
        case (.decodeFailed(let l), .decodeFailed(let r)):
            return l == r
        case (.seekFailed(let l), .seekFailed(let r)):
            return l == r
        case (.memoryError, .memoryError):
            return true
        case (.notPrepared, .notPrepared):
            return true
        case (.invalidHandle, .invalidHandle):
            return true
        case (.alreadyInvalidated, .alreadyInvalidated):
            return true
        case (.cancelled, .cancelled):
            return true
        case (.rustPanic(let l), .rustPanic(let r)):
            return l == r
        case (.unknown(let l), .unknown(let r)):
            return l == r
        default:
            return false
        }
    }
}
