// FFmpegTimecodeTests.swift
// CYBFFmpeg
//
// Tests for the SMPTE timecode bridge types added in Phase 2-F.

import XCTest
@testable import CYBFFmpeg

final class FFmpegTimecodeTests: XCTestCase {
    // MARK: - FFmpegFrameRate

    func test_frameRate_asDouble_lossyForNTSC() {
        let r = FFmpegFrameRate(numerator: 24000, denominator: 1001)
        XCTAssertEqual(r.asDouble, 24000.0 / 1001.0, accuracy: 1e-9)
        XCTAssertTrue(r.isNTSC)
    }

    func test_frameRate_asDouble_zeroDenominator() {
        let r = FFmpegFrameRate(numerator: 24000, denominator: 0)
        XCTAssertEqual(r.asDouble, 0)
    }

    // MARK: - SMPTEArithmetic.tcIntegerRate

    func test_tcIntegerRate_collapsesNTSC() {
        XCTAssertEqual(
            SMPTEArithmetic.tcIntegerRate(.init(numerator: 24000, denominator: 1001)),
            24
        )
        XCTAssertEqual(
            SMPTEArithmetic.tcIntegerRate(.init(numerator: 30000, denominator: 1001)),
            30
        )
        XCTAssertEqual(
            SMPTEArithmetic.tcIntegerRate(.init(numerator: 60000, denominator: 1001)),
            60
        )
        XCTAssertEqual(
            SMPTEArithmetic.tcIntegerRate(.init(numerator: 25, denominator: 1)),
            25
        )
    }

    // MARK: - displayString round-trips

    func test_displayString_24fps_oneHour() {
        let tc = FFmpegTimecode(
            frameNumber: 86_400,
            rate: .init(numerator: 24, denominator: 1),
            dropFrame: false,
            sourceKind: .containerMetadata,
            sourceDetail: "test",
            confidence: 1.0
        )
        XCTAssertEqual(tc.displayString, "01:00:00:00")
    }

    func test_displayString_2997DF_oneHour() {
        // 29.97 DF: 1 hour = 107892 frames (per SMPTE 12M reference).
        let tc = FFmpegTimecode(
            frameNumber: 107_892,
            rate: .init(numerator: 30000, denominator: 1001),
            dropFrame: true,
            sourceKind: .tmcdTrack,
            sourceDetail: "test",
            confidence: 0.95
        )
        XCTAssertEqual(tc.displayString, "01:00:00;00")
    }

    func test_displayString_2997DF_minuteOneDropsTwoFrames() {
        // Frame 1800 in 29.97 DF should be "00:01:00;02".
        let tc = FFmpegTimecode(
            frameNumber: 1_800,
            rate: .init(numerator: 30000, denominator: 1001),
            dropFrame: true,
            sourceKind: .tmcdTrack,
            sourceDetail: "test",
            confidence: 0.95
        )
        XCTAssertEqual(tc.displayString, "00:01:00;02")
    }

    func test_displayString_2997DF_tenthMinuteNoDrop() {
        // Frame 17982 in 29.97 DF == "00:10:00;00".
        let tc = FFmpegTimecode(
            frameNumber: 17_982,
            rate: .init(numerator: 30000, denominator: 1001),
            dropFrame: true,
            sourceKind: .tmcdTrack,
            sourceDetail: "test",
            confidence: 0.95
        )
        XCTAssertEqual(tc.displayString, "00:10:00;00")
    }

    func test_displayString_5994DF_oneHour() {
        // 59.94 DF: 1 hour = 215784 frames.
        let tc = FFmpegTimecode(
            frameNumber: 215_784,
            rate: .init(numerator: 60000, denominator: 1001),
            dropFrame: true,
            sourceKind: .tmcdTrack,
            sourceDetail: "test",
            confidence: 0.95
        )
        XCTAssertEqual(tc.displayString, "01:00:00;00")
    }

    func test_displayString_clampsNegativeFrames() {
        let tc = FFmpegTimecode(
            frameNumber: -10,
            rate: .init(numerator: 24, denominator: 1),
            dropFrame: false,
            sourceKind: .inferred,
            sourceDetail: "",
            confidence: 0.1
        )
        XCTAssertEqual(tc.displayString, "00:00:00:00")
    }

    // MARK: - seconds (lossy bridge)

    func test_seconds_23976_oneHour() {
        // "01:00:00:00" @ 23.976 = 86400 frames * 1001/24000 ≈ 3603.6s
        let tc = FFmpegTimecode(
            frameNumber: 86_400,
            rate: .init(numerator: 24000, denominator: 1001),
            dropFrame: false,
            sourceKind: .tmcdTrack,
            sourceDetail: "",
            confidence: 0.95
        )
        XCTAssertEqual(tc.seconds, 3603.6, accuracy: 0.001)
    }

    // MARK: - Codable round-trip

    func test_codable_roundTrip() throws {
        let original = FFmpegTimecode(
            frameNumber: 1_087_104,
            rate: .init(numerator: 24000, denominator: 1001),
            dropFrame: false,
            sourceKind: .mxfMaterialPackage,
            sourceDetail: "ffmpeg:mxf:format:timecode",
            confidence: 0.9
        )
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(FFmpegTimecode.self, from: data)
        XCTAssertEqual(decoded, original)
    }
}
