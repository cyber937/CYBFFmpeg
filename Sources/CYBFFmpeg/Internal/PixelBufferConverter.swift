// PixelBufferConverter.swift
// CYBFFmpeg
//
// Converts raw pixel data from Rust to CVPixelBuffer.

import Foundation
import CoreVideo
import VideoToolbox

// MARK: - PixelBufferConverter

/// Converts raw pixel data to CVPixelBuffer for Metal rendering
internal enum PixelBufferConverter {
    // MARK: - Pixel Buffer Pool

    // Using nonisolated(unsafe) for static mutable state protected by lock
    nonisolated(unsafe) private static var bufferPool: CVPixelBufferPool?
    nonisolated(unsafe) private static var poolWidth: Int = 0
    nonisolated(unsafe) private static var poolHeight: Int = 0
    nonisolated(unsafe) private static var poolFormat: OSType = 0
    private static let poolLock = NSLock()

    // MARK: - Conversion

    /// Convert raw pixel data to CVPixelBuffer
    /// - Parameters:
    ///   - data: Pointer to raw pixel data
    ///   - width: Frame width
    ///   - height: Frame height
    ///   - stride: Bytes per row
    ///   - format: Pixel format
    ///   - dataSize: Total data size in bytes
    /// - Returns: CVPixelBuffer suitable for Metal rendering
    static func convert(
        data: UnsafePointer<UInt8>?,
        width: Int,
        height: Int,
        stride: Int,
        format: PixelFormat,
        dataSize: Int
    ) throws -> CVPixelBuffer {
        guard let data = data else {
            throw FFmpegError.memoryError
        }

        // Convert to raw pointer for memcpy operations
        let rawData = UnsafeRawPointer(data)

        let pixelFormat = format.cvPixelFormat

        // Get or create buffer pool
        let pool = try getOrCreatePool(width: width, height: height, format: pixelFormat)

        // Create pixel buffer from pool
        var pixelBuffer: CVPixelBuffer?
        let status = CVPixelBufferPoolCreatePixelBuffer(nil, pool, &pixelBuffer)

        guard status == kCVReturnSuccess, let buffer = pixelBuffer else {
            throw FFmpegError.memoryError
        }

        // Lock buffer for writing
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }

        // Copy data based on format
        switch format {
        case .bgra:
            try copyBGRA(from: rawData, to: buffer, width: width, height: height, srcStride: stride)

        case .nv12:
            try copyNV12(from: rawData, to: buffer, width: width, height: height, srcStride: stride)

        case .yuv420p:
            try copyYUV420P(from: rawData, to: buffer, width: width, height: height, srcStride: stride)
        }

        return buffer
    }

    // MARK: - Buffer Pool Management

    private static func getOrCreatePool(width: Int, height: Int, format: OSType) throws -> CVPixelBufferPool {
        poolLock.lock()
        defer { poolLock.unlock() }

        // Return existing pool if compatible
        if let pool = bufferPool,
           poolWidth == width,
           poolHeight == height,
           poolFormat == format {
            return pool
        }

        // Create new pool
        let poolAttributes: [String: Any] = [
            kCVPixelBufferPoolMinimumBufferCountKey as String: 3
        ]

        let pixelBufferAttributes: [String: Any] = [
            kCVPixelBufferPixelFormatTypeKey as String: format,
            kCVPixelBufferWidthKey as String: width,
            kCVPixelBufferHeightKey as String: height,
            kCVPixelBufferIOSurfacePropertiesKey as String: [:],
            kCVPixelBufferMetalCompatibilityKey as String: true
        ]

        var pool: CVPixelBufferPool?
        let status = CVPixelBufferPoolCreate(
            nil,
            poolAttributes as CFDictionary,
            pixelBufferAttributes as CFDictionary,
            &pool
        )

        guard status == kCVReturnSuccess, let newPool = pool else {
            throw FFmpegError.memoryError
        }

        // Update pool tracking
        bufferPool = newPool
        poolWidth = width
        poolHeight = height
        poolFormat = format

        return newPool
    }

    // MARK: - Copy Functions

    private static func copyBGRA(
        from src: UnsafeRawPointer,
        to dst: CVPixelBuffer,
        width: Int,
        height: Int,
        srcStride: Int
    ) throws {
        guard let baseAddress = CVPixelBufferGetBaseAddress(dst) else {
            throw FFmpegError.memoryError
        }

        let dstStride = CVPixelBufferGetBytesPerRow(dst)

        if srcStride == dstStride {
            // Fast path: direct copy
            memcpy(baseAddress, src, height * srcStride)
        } else {
            // Row-by-row copy
            for y in 0..<height {
                let srcRow = src.advanced(by: y * srcStride)
                let dstRow = baseAddress.advanced(by: y * dstStride)
                memcpy(dstRow, srcRow, min(srcStride, dstStride))
            }
        }
    }

    private static func copyNV12(
        from src: UnsafeRawPointer,
        to dst: CVPixelBuffer,
        width: Int,
        height: Int,
        srcStride: Int
    ) throws {
        // NV12 is planar: Y plane + UV interleaved plane
        guard CVPixelBufferGetPlaneCount(dst) >= 2 else {
            throw FFmpegError.invalidFormat("Expected planar buffer for NV12")
        }

        // Copy Y plane
        guard let yPlane = CVPixelBufferGetBaseAddressOfPlane(dst, 0) else {
            throw FFmpegError.memoryError
        }
        let yStride = CVPixelBufferGetBytesPerRowOfPlane(dst, 0)
        let yHeight = CVPixelBufferGetHeightOfPlane(dst, 0)

        for y in 0..<yHeight {
            let srcRow = src.advanced(by: y * srcStride)
            let dstRow = yPlane.advanced(by: y * yStride)
            memcpy(dstRow, srcRow, min(srcStride, yStride))
        }

        // Copy UV plane
        guard let uvPlane = CVPixelBufferGetBaseAddressOfPlane(dst, 1) else {
            throw FFmpegError.memoryError
        }
        let uvStride = CVPixelBufferGetBytesPerRowOfPlane(dst, 1)
        let uvHeight = CVPixelBufferGetHeightOfPlane(dst, 1)
        let uvOffset = height * srcStride

        for y in 0..<uvHeight {
            let srcRow = src.advanced(by: uvOffset + y * srcStride)
            let dstRow = uvPlane.advanced(by: y * uvStride)
            memcpy(dstRow, srcRow, min(srcStride, uvStride))
        }
    }

    private static func copyYUV420P(
        from src: UnsafeRawPointer,
        to dst: CVPixelBuffer,
        width: Int,
        height: Int,
        srcStride: Int
    ) throws {
        // YUV420P: Y plane + U plane + V plane (all separate)
        // For simplicity, convert to NV12 or BGRA if needed
        // This is a placeholder - actual implementation would handle planar format

        guard CVPixelBufferGetPlaneCount(dst) >= 2 else {
            throw FFmpegError.invalidFormat("Expected planar buffer for YUV420P")
        }

        // Similar to NV12 but with separate U and V planes
        // For now, copy Y plane only (partial implementation)
        guard let yPlane = CVPixelBufferGetBaseAddressOfPlane(dst, 0) else {
            throw FFmpegError.memoryError
        }
        let yStride = CVPixelBufferGetBytesPerRowOfPlane(dst, 0)

        for y in 0..<height {
            let srcRow = src.advanced(by: y * srcStride)
            let dstRow = yPlane.advanced(by: y * yStride)
            memcpy(dstRow, srcRow, min(srcStride, yStride))
        }

        // U and V planes would need interleaving to NV12 format
        // This is left as a TODO for full YUV420P support
    }

    // MARK: - Cleanup

    /// Release the buffer pool (call when done with decoder)
    static func releasePool() {
        poolLock.lock()
        defer { poolLock.unlock() }

        bufferPool = nil
        poolWidth = 0
        poolHeight = 0
        poolFormat = 0
    }
}

// MARK: - PixelFormat Extensions

extension PixelFormat {
    /// CoreVideo pixel format type
    var cvPixelFormat: OSType {
        switch self {
        case .bgra:
            return kCVPixelFormatType_32BGRA
        case .nv12:
            return kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
        case .yuv420p:
            return kCVPixelFormatType_420YpCbCr8Planar
        }
    }
}
