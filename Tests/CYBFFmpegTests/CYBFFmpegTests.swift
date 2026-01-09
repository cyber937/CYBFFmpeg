// CYBFFmpegTests.swift
// CYBFFmpeg
//
// Basic tests for CYBFFmpeg package.

import XCTest
@testable import CYBFFmpeg

final class CYBFFmpegTests: XCTestCase {
    // MARK: - Test Sample Path Helper

    /// Returns the path to sample files for testing.
    /// Set CYBFFMPEG_SAMPLES_PATH environment variable to override.
    private func samplePath(_ filename: String) -> URL {
        if let envPath = ProcessInfo.processInfo.environment["CYBFFMPEG_SAMPLES_PATH"] {
            return URL(fileURLWithPath: envPath).appendingPathComponent(filename)
        }
        // Default: samples directory relative to package root
        let packageRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // CYBFFmpegTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // Package root
        return packageRoot.appendingPathComponent("samples").appendingPathComponent(filename)
    }

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

    // MARK: - Video Decoding Tests

    func testVideoDecodingMKV() async throws {
        // Path to the sample MKV file
        let samplePath = samplePath("sample_1280x720_surfing_with_audio.mkv")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        // Create decoder
        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        // Verify media info
        guard let mediaInfo = decoder.mediaInfo else {
            XCTFail("Media info should be available after prepare")
            return
        }

        XCTAssertGreaterThan(mediaInfo.duration, 0, "Duration should be positive")
        XCTAssertFalse(mediaInfo.videoTracks.isEmpty, "MKV should have video tracks")

        if let videoTrack = mediaInfo.videoTracks.first {
            XCTAssertEqual(videoTrack.width, 1280, "Video width should be 1280")
            XCTAssertEqual(videoTrack.height, 720, "Video height should be 720")
            XCTAssertGreaterThan(videoTrack.frameRate, 0, "Frame rate should be positive")
            print("Video: \(videoTrack.width)x\(videoTrack.height) @ \(videoTrack.frameRate)fps, codec: \(videoTrack.codec.name)")
        }

        // Start decoding
        decoder.startDecoding()

        // Decode a few frames
        var frameCount = 0
        var lastPTS: Double = -1

        for _ in 0..<5 {
            if let frame = decoder.getNextFrame() {
                XCTAssertEqual(frame.width, 1280, "Frame width should match")
                XCTAssertEqual(frame.height, 720, "Frame height should match")
                XCTAssertNotNil(frame.pixelBuffer, "Frame should have pixel buffer")
                XCTAssertGreaterThanOrEqual(frame.presentationTime, lastPTS, "PTS should be monotonically increasing")

                lastPTS = frame.presentationTime
                frameCount += 1

                print("Frame \(frameCount): \(frame.width)x\(frame.height), pts=\(frame.presentationTime)s, keyframe=\(frame.isKeyframe)")
            }
        }

        XCTAssertGreaterThan(frameCount, 0, "Should have decoded at least one video frame")

        // Cleanup
        decoder.stopDecoding()
        decoder.invalidate()
    }

    func testVideoDecodingWebM() async throws {
        // Path to the sample WebM file (VP9)
        let samplePath = samplePath("sample_960x400_ocean_with_audio.webm")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        // Create decoder
        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        // Verify media info
        guard let mediaInfo = decoder.mediaInfo else {
            XCTFail("Media info should be available after prepare")
            return
        }

        XCTAssertGreaterThan(mediaInfo.duration, 0, "Duration should be positive")
        XCTAssertFalse(mediaInfo.videoTracks.isEmpty, "WebM should have video tracks")

        if let videoTrack = mediaInfo.videoTracks.first {
            // VP9 codec
            XCTAssertTrue(videoTrack.codec.name.contains("vp9") || videoTrack.codec.name.contains("vp8"),
                          "WebM should use VP8/VP9 codec, got: \(videoTrack.codec.name)")
            print("Video: \(videoTrack.width)x\(videoTrack.height), codec: \(videoTrack.codec.name)")
        }

        // Start decoding
        decoder.startDecoding()

        // Decode a few frames
        var frameCount = 0

        for _ in 0..<3 {
            if let frame = decoder.getNextFrame() {
                XCTAssertNotNil(frame.pixelBuffer, "Frame should have pixel buffer")
                frameCount += 1
                print("Frame \(frameCount): \(frame.width)x\(frame.height), pts=\(frame.presentationTime)s")
            }
        }

        XCTAssertGreaterThan(frameCount, 0, "Should have decoded at least one VP9 frame")

        // Cleanup
        decoder.stopDecoding()
        decoder.invalidate()
    }

    // MARK: - Seek Tests

    func testSeekToMiddle() async throws {
        // Path to the sample MKV file
        let samplePath = samplePath("sample_1280x720_surfing_with_audio.mkv")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        guard let mediaInfo = decoder.mediaInfo else {
            XCTFail("Media info should be available")
            return
        }

        let duration = mediaInfo.duration
        let targetTime = duration / 2.0  // Seek to middle

        print("Duration: \(duration)s, seeking to: \(targetTime)s")

        // Seek to middle
        if let frame = try decoder.seek(to: targetTime) {
            let pts = frame.presentationTime
            let diff = abs(pts - targetTime)

            print("Seeked to: \(pts)s (target: \(targetTime)s, diff: \(diff)s)")

            // Allow tolerance of a few frames
            XCTAssertLessThan(diff, 1.0, "Seek should land within 1 second of target")
            XCTAssertNotNil(frame.pixelBuffer, "Frame should have pixel buffer after seek")
        } else {
            XCTFail("Seek should return a frame")
        }

        decoder.invalidate()
    }

    func testSeekToBeginning() async throws {
        // Path to the sample file
        let samplePath = samplePath("sample_960x400_ocean_with_audio.wmv")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        // First, decode a few frames
        decoder.startDecoding()
        for _ in 0..<5 {
            _ = decoder.getNextFrame()
        }
        decoder.stopDecoding()

        // Now seek back to beginning
        if let frame = try decoder.seek(to: 0.0) {
            let pts = frame.presentationTime

            print("Seeked to beginning: pts=\(pts)s")

            // Should be near the start (first frame might not be exactly 0)
            XCTAssertLessThan(pts, 0.5, "Seek to 0 should land near the beginning")
            XCTAssertNotNil(frame.pixelBuffer, "Frame should have pixel buffer")
        } else {
            XCTFail("Seek to beginning should return a frame")
        }

        decoder.invalidate()
    }

    func testSeekNearEnd() async throws {
        // Path to the sample file
        let samplePath = samplePath("sample_960x400_ocean_with_audio.wmv")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        guard let mediaInfo = decoder.mediaInfo else {
            XCTFail("Media info should be available")
            return
        }

        let duration = mediaInfo.duration
        let targetTime = duration - 1.0  // 1 second before end

        print("Duration: \(duration)s, seeking to: \(targetTime)s")

        if let frame = try decoder.seek(to: targetTime) {
            let pts = frame.presentationTime

            print("Seeked near end: pts=\(pts)s")

            // Should be within reasonable range
            XCTAssertGreaterThan(pts, duration - 2.0, "Should be near the end")
            XCTAssertNotNil(frame.pixelBuffer, "Frame should have pixel buffer")
        } else {
            // Seeking near end might not always succeed depending on file structure
            print("Seek near end returned nil (acceptable for some formats)")
        }

        decoder.invalidate()
    }

    func testSeekMultipleTimes() async throws {
        // Path to the sample file
        let samplePath = samplePath("sample_1280x720_surfing_with_audio.mkv")

        guard FileManager.default.fileExists(atPath: samplePath.path) else {
            print("Skipping test: sample file not found at \(samplePath.path)")
            return
        }

        let decoder = try FFmpegDecoder(url: samplePath)
        try await decoder.prepare()

        guard let mediaInfo = decoder.mediaInfo else {
            XCTFail("Media info should be available")
            return
        }

        let duration = mediaInfo.duration
        let seekTimes = [0.0, duration * 0.25, duration * 0.5, duration * 0.75, 0.0]
        var successCount = 0

        for targetTime in seekTimes {
            if let frame = try decoder.seek(to: targetTime) {
                print("Seek to \(targetTime)s -> got frame at \(frame.presentationTime)s")
                successCount += 1
            } else {
                print("Seek to \(targetTime)s -> nil")
            }
        }

        XCTAssertEqual(successCount, seekTimes.count, "All seeks should succeed")

        decoder.invalidate()
    }

    // MARK: - Audio Decoding Tests

    func testAudioDecodingWMV() async throws {
        // Path to the sample WMV file
        let samplePath = samplePath("sample_960x400_ocean_with_audio.wmv")

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
