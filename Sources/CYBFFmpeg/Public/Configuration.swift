// Configuration.swift
// CYBFFmpeg
//
// Decoder and cache configuration types.

import Foundation

// MARK: - DecoderConfiguration

/// Configuration options for FFmpegDecoder
public struct DecoderConfiguration: Sendable {
    /// Whether to prefer hardware decoding via VideoToolbox
    public let preferHardwareDecoding: Bool

    /// Cache configuration
    public let cacheConfiguration: CacheConfiguration

    /// Number of decoding threads (0 = auto)
    public let threadCount: Int

    /// Output pixel format for decoded frames
    public let outputPixelFormat: PixelFormat

    // MARK: Initialization

    /// Create a custom configuration
    public init(
        preferHardwareDecoding: Bool = true,
        cacheConfiguration: CacheConfiguration = .default,
        threadCount: Int = 0,
        outputPixelFormat: PixelFormat = .bgra
    ) {
        self.preferHardwareDecoding = preferHardwareDecoding
        self.cacheConfiguration = cacheConfiguration
        self.threadCount = threadCount
        self.outputPixelFormat = outputPixelFormat
    }

    // MARK: Presets

    /// Default configuration suitable for most use cases
    public static let `default` = DecoderConfiguration()

    /// Performance-optimized configuration with larger caches
    public static let performance = DecoderConfiguration(
        preferHardwareDecoding: true,
        cacheConfiguration: .performance,
        threadCount: 0,
        outputPixelFormat: .bgra
    )

    /// Low memory configuration with smaller caches
    public static let lowMemory = DecoderConfiguration(
        preferHardwareDecoding: true,
        cacheConfiguration: .lowMemory,
        threadCount: 2,
        outputPixelFormat: .nv12
    )

    /// Scrubbing-optimized configuration
    public static let scrubbing = DecoderConfiguration(
        preferHardwareDecoding: true,
        cacheConfiguration: .scrubbing,
        threadCount: 0,
        outputPixelFormat: .bgra
    )
}

// MARK: - CacheConfiguration

/// Configuration for the multi-tier frame cache
public struct CacheConfiguration: Sendable {
    /// L1 (Hot) cache capacity - recent frames
    public let l1Capacity: Int

    /// L2 (Keyframe) cache capacity - keyframes only
    public let l2Capacity: Int

    /// L3 (Cold) cache capacity - prefetched frames
    public let l3Capacity: Int

    /// Whether to enable background prefetching
    public let enablePrefetch: Bool

    // MARK: Initialization

    /// Create a custom cache configuration
    public init(
        l1Capacity: Int = 30,
        l2Capacity: Int = 100,
        l3Capacity: Int = 500,
        enablePrefetch: Bool = true
    ) {
        precondition(l1Capacity >= 0, "L1 capacity must be non-negative")
        precondition(l2Capacity >= 0, "L2 capacity must be non-negative")
        precondition(l3Capacity >= 0, "L3 capacity must be non-negative")

        self.l1Capacity = l1Capacity
        self.l2Capacity = l2Capacity
        self.l3Capacity = l3Capacity
        self.enablePrefetch = enablePrefetch
    }

    // MARK: Presets

    /// Default cache configuration
    public static let `default` = CacheConfiguration()

    /// Performance configuration with larger caches
    public static let performance = CacheConfiguration(
        l1Capacity: 60,
        l2Capacity: 200,
        l3Capacity: 1000,
        enablePrefetch: true
    )

    /// Low memory configuration
    public static let lowMemory = CacheConfiguration(
        l1Capacity: 15,
        l2Capacity: 50,
        l3Capacity: 100,
        enablePrefetch: false
    )

    /// Scrubbing-optimized configuration
    public static let scrubbing = CacheConfiguration(
        l1Capacity: 45,
        l2Capacity: 200,
        l3Capacity: 800,
        enablePrefetch: true
    )

    /// No caching (for debugging)
    public static let disabled = CacheConfiguration(
        l1Capacity: 0,
        l2Capacity: 0,
        l3Capacity: 0,
        enablePrefetch: false
    )

    // MARK: Computed Properties

    /// Total maximum cache capacity
    public var totalCapacity: Int {
        l1Capacity + l2Capacity + l3Capacity
    }

    /// Estimated maximum memory usage (rough estimate for 1080p BGRA)
    public var estimatedMaxMemoryMB: Int {
        // 1080p BGRA = ~8MB per frame
        let bytesPerFrame = 1920 * 1080 * 4
        return (totalCapacity * bytesPerFrame) / (1024 * 1024)
    }
}

// MARK: - PixelFormat

/// Output pixel format for decoded frames
public enum PixelFormat: String, Sendable, Codable, CaseIterable {
    /// BGRA format - best for Metal rendering
    case bgra

    /// NV12 format - VideoToolbox native, lower memory
    case nv12

    /// YUV420P format - software decode native
    case yuv420p

    /// Human-readable description
    public var description: String {
        switch self {
        case .bgra:
            return "BGRA (32-bit, Metal optimized)"
        case .nv12:
            return "NV12 (12-bit, VideoToolbox native)"
        case .yuv420p:
            return "YUV420P (12-bit, planar)"
        }
    }

    /// Bytes per pixel (for non-planar formats)
    public var bytesPerPixel: Int {
        switch self {
        case .bgra:
            return 4
        case .nv12, .yuv420p:
            return 1 // Actually 1.5 but stored as planes
        }
    }

    /// Whether this is a planar format
    public var isPlanar: Bool {
        switch self {
        case .bgra:
            return false
        case .nv12, .yuv420p:
            return true
        }
    }
}

// MARK: - CacheStatistics

/// Statistics about cache performance
public struct CacheStatistics: Sendable {
    /// Number of entries in L1 cache
    public let l1Entries: Int

    /// Number of entries in L2 cache
    public let l2Entries: Int

    /// Number of entries in L3 cache
    public let l3Entries: Int

    /// Number of L1 cache hits
    public let l1HitCount: Int

    /// Number of L2 cache hits
    public let l2HitCount: Int

    /// Number of L3 cache hits
    public let l3HitCount: Int

    /// Number of cache misses
    public let missCount: Int

    /// Total memory usage in bytes
    public let memoryUsageBytes: Int

    // MARK: Computed Properties

    /// Total entries across all cache tiers
    public var totalEntries: Int {
        l1Entries + l2Entries + l3Entries
    }

    /// Total cache accesses
    public var totalAccesses: Int {
        l1HitCount + l2HitCount + l3HitCount + missCount
    }

    /// Overall cache hit rate (0.0 - 1.0)
    public var hitRate: Double {
        guard totalAccesses > 0 else { return 0 }
        return Double(l1HitCount + l2HitCount + l3HitCount) / Double(totalAccesses)
    }

    /// L1 hit rate
    public var l1HitRate: Double {
        guard totalAccesses > 0 else { return 0 }
        return Double(l1HitCount) / Double(totalAccesses)
    }

    /// L2 hit rate
    public var l2HitRate: Double {
        guard totalAccesses > 0 else { return 0 }
        return Double(l2HitCount) / Double(totalAccesses)
    }

    /// L3 hit rate
    public var l3HitRate: Double {
        guard totalAccesses > 0 else { return 0 }
        return Double(l3HitCount) / Double(totalAccesses)
    }

    /// Miss rate
    public var missRate: Double {
        guard totalAccesses > 0 else { return 0 }
        return Double(missCount) / Double(totalAccesses)
    }

    /// Memory usage in megabytes
    public var memoryUsageMB: Double {
        Double(memoryUsageBytes) / (1024 * 1024)
    }

    // MARK: Initialization

    /// Internal initializer
    internal init(
        l1Entries: Int,
        l2Entries: Int,
        l3Entries: Int,
        l1HitCount: Int,
        l2HitCount: Int,
        l3HitCount: Int,
        missCount: Int,
        memoryUsageBytes: Int
    ) {
        self.l1Entries = l1Entries
        self.l2Entries = l2Entries
        self.l3Entries = l3Entries
        self.l1HitCount = l1HitCount
        self.l2HitCount = l2HitCount
        self.l3HitCount = l3HitCount
        self.missCount = missCount
        self.memoryUsageBytes = memoryUsageBytes
    }

    /// Empty statistics
    public static let empty = CacheStatistics(
        l1Entries: 0,
        l2Entries: 0,
        l3Entries: 0,
        l1HitCount: 0,
        l2HitCount: 0,
        l3HitCount: 0,
        missCount: 0,
        memoryUsageBytes: 0
    )
}

// MARK: - CustomStringConvertible

extension CacheStatistics: CustomStringConvertible {
    public var description: String {
        """
        CacheStatistics(
            entries: L1=\(l1Entries), L2=\(l2Entries), L3=\(l3Entries),
            hitRate: \(String(format: "%.1f%%", hitRate * 100)),
            memory: \(String(format: "%.1f", memoryUsageMB)) MB
        )
        """
    }
}

// MARK: - Codable Conformance

extension DecoderConfiguration: Codable {}
extension CacheConfiguration: Codable {}
extension CacheStatistics: Codable {}
