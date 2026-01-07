// CYBFFmpegTests.swift
// CYBFFmpeg
//
// Basic tests for CYBFFmpeg package.

import XCTest
@testable import CYBFFmpeg

final class CYBFFmpegTests: XCTestCase {
    // MARK: - Configuration Tests

    func testDefaultConfiguration() {
        let config = DecoderConfiguration.default

        XCTAssertTrue(config.preferHardwareDecoding)
        XCTAssertEqual(config.threadCount, 0)  // Auto-detect
        XCTAssertEqual(config.outputPixelFormat, .bgra)
        XCTAssertTrue(config.cacheConfiguration.enablePrefetch)
    }

    func testCacheConfiguration() {
        let cacheConfig = CacheConfiguration(
            l1Capacity: 50,
            l2Capacity: 150,
            l3Capacity: 600,
            enablePrefetch: false
        )

        XCTAssertEqual(cacheConfig.l1Capacity, 50)
        XCTAssertEqual(cacheConfig.l2Capacity, 150)
        XCTAssertEqual(cacheConfig.l3Capacity, 600)
        XCTAssertFalse(cacheConfig.enablePrefetch)
    }

    // MARK: - Error Tests

    func testFFmpegErrorDescriptions() {
        let fileNotFoundError = FFmpegError.fileNotFound(URL(fileURLWithPath: "/test.mp4"))
        XCTAssertTrue(fileNotFoundError.localizedDescription.contains("test.mp4"))

        let invalidFormatError = FFmpegError.invalidFormat("Unknown format")
        XCTAssertTrue(invalidFormatError.localizedDescription.contains("Unknown format"))

        let codecError = FFmpegError.codecNotSupported("test_codec")
        XCTAssertTrue(codecError.localizedDescription.contains("test_codec"))
    }

    // MARK: - Cache Statistics Tests

    func testCacheStatisticsEmpty() {
        let stats = CacheStatistics.empty

        XCTAssertEqual(stats.l1Entries, 0)
        XCTAssertEqual(stats.l2Entries, 0)
        XCTAssertEqual(stats.l3Entries, 0)
        XCTAssertEqual(stats.l1HitCount, 0)
        XCTAssertEqual(stats.l2HitCount, 0)
        XCTAssertEqual(stats.l3HitCount, 0)
        XCTAssertEqual(stats.missCount, 0)
        XCTAssertEqual(stats.memoryUsageBytes, 0)
    }

    func testCacheStatisticsHitRate() {
        let stats = CacheStatistics(
            l1Entries: 10,
            l2Entries: 50,
            l3Entries: 200,
            l1HitCount: 80,
            l2HitCount: 15,
            l3HitCount: 5,
            missCount: 0,
            memoryUsageBytes: 1024 * 1024
        )

        // Total hits: 80 + 15 + 5 = 100
        // Total requests: 100 + 0 = 100
        // Hit rate: 100%
        XCTAssertEqual(stats.hitRate, 1.0, accuracy: 0.001)

        // With some misses
        let statsWithMisses = CacheStatistics(
            l1Entries: 10,
            l2Entries: 50,
            l3Entries: 200,
            l1HitCount: 70,
            l2HitCount: 10,
            l3HitCount: 10,
            missCount: 10,
            memoryUsageBytes: 1024 * 1024
        )

        // Total hits: 70 + 10 + 10 = 90
        // Total requests: 90 + 10 = 100
        // Hit rate: 90%
        XCTAssertEqual(statsWithMisses.hitRate, 0.9, accuracy: 0.001)
    }

    // MARK: - Media Info Tests

    func testFFmpegCodec() {
        let codec = FFmpegCodec(name: "vp9", longName: "Google VP9", fourCC: "vp09")

        XCTAssertEqual(codec.name, "vp9")
        XCTAssertEqual(codec.longName, "Google VP9")
        XCTAssertEqual(codec.fourCC, "vp09")
    }

    func testFFmpegVideoTrack() {
        let codec = FFmpegCodec(name: "av1", longName: "AV1", fourCC: nil)
        let track = FFmpegVideoTrack(
            index: 0,
            codec: codec,
            width: 1920,
            height: 1080,
            frameRate: 30.0,
            bitRate: 5_000_000,
            pixelFormat: "yuv420p",
            isHardwareDecodable: true,
            colorSpace: nil,
            colorPrimaries: nil,
            colorTransfer: nil,
            colorRange: .full
        )

        XCTAssertEqual(track.index, 0)
        XCTAssertEqual(track.codec.name, "av1")
        XCTAssertEqual(track.width, 1920)
        XCTAssertEqual(track.height, 1080)
        XCTAssertEqual(track.frameRate, 30.0)
        XCTAssertEqual(track.bitRate, 5_000_000)
        XCTAssertTrue(track.isHardwareDecodable)
    }

    func testFFmpegAudioTrack() {
        let codec = FFmpegCodec(name: "opus", longName: "Opus", fourCC: nil)
        let track = FFmpegAudioTrack(
            index: 1,
            codec: codec,
            sampleRate: 48000,
            channels: 2,
            channelLayout: "stereo",
            bitRate: 128_000,
            languageCode: "eng"
        )

        XCTAssertEqual(track.index, 1)
        XCTAssertEqual(track.codec.name, "opus")
        XCTAssertEqual(track.sampleRate, 48000)
        XCTAssertEqual(track.channels, 2)
        XCTAssertEqual(track.channelLayout, "stereo")
    }

    // MARK: - Pixel Format Tests

    func testPixelFormatConversion() {
        XCTAssertEqual(PixelFormat.bgra.toCybFormat(), 0)
        XCTAssertEqual(PixelFormat.nv12.toCybFormat(), 1)
        XCTAssertEqual(PixelFormat.yuv420p.toCybFormat(), 2)

        XCTAssertEqual(PixelFormat(rawValue: 0), .bgra)
        XCTAssertEqual(PixelFormat(rawValue: 1), .nv12)
        XCTAssertEqual(PixelFormat(rawValue: 2), .yuv420p)
        XCTAssertEqual(PixelFormat(rawValue: 99), .bgra)  // Unknown defaults to bgra
    }

    // MARK: - Audio Decoding Tests

    func testAudioDecodingWMV() async throws {
        // Path to the sample WMV file (absolute path for reliability)
        let samplePath = URL(fileURLWithPath: "PACKAGE_ROOT_PATH/samples/sample_960x400_ocean_with_audio.wmv")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        // Create decoder
        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        // Verify audio is available
        XCTAssertTrue(decoder.hasAudio, "WMV file should have audio")
        XCTAssertGreaterThan(decoder.audioSampleRate, 0, "Sample rate should be positive")
        XCTAssertGreaterThan(decoder.audioChannels, 0, "Channel count should be positive")

        print("Audio info: \(decoder.audioSampleRate)Hz, \(decoder.audioChannels) channels")

        // Start decoding
        decoder.startDecoding()

        // Try to decode a few audio frames
        var frameCount = 0
        var totalSamples = 0

        for _ in 0..<10 {
            if let frame = decoder.getNextAudioFrame() {
                XCTAssertGreaterThan(frame.sampleCount, 0, "Frame should have samples")
                XCTAssertEqual(frame.channels, decoder.audioChannels, "Channel count should match")
                XCTAssertEqual(frame.sampleRate, decoder.audioSampleRate, "Sample rate should match")
                XCTAssertEqual(frame.samples.count, frame.sampleCount * frame.channels, "Sample array size should match")

                frameCount += 1
                totalSamples += frame.sampleCount

                print("Frame \(frameCount): \(frame.sampleCount) samples, pts=\(frame.presentationTime)s")
            }
        }

        XCTAssertGreaterThan(frameCount, 0, "Should have decoded at least one audio frame")
        XCTAssertGreaterThan(totalSamples, 0, "Should have decoded some audio samples")

        print("Decoded \(frameCount) frames with \(totalSamples) total samples")

        // Cleanup
        decoder.stopDecoding()
        decoder.invalidate()
    }
}
