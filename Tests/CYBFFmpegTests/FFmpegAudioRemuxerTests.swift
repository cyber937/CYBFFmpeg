// FFmpegAudioRemuxerTests.swift
// CYBFFmpeg
//
// 音声ストリームの無変換コピー（kirinuki-ai #1765）。

import XCTest
@testable import CYBFFmpeg

/// 🚨 **実素材が要るテストは、素材が無ければ skip する。**
/// CI にも他の開発機にも同じ動画は置けないので、`CYB_REMUX_TEST_SOURCE` で渡す。
final class FFmpegAudioRemuxerTests: XCTestCase {

    private var sourceURL: URL? {
        guard let p = ProcessInfo.processInfo.environment["CYB_REMUX_TEST_SOURCE"],
              FileManager.default.fileExists(atPath: p) else { return nil }
        return URL(fileURLWithPath: p)
    }

    /// 音声が無いファイルでは `nil`（例外にしない）。
    func testAudioCodecNameIsNilWithoutAudio() throws {
        let empty = FileManager.default.temporaryDirectory
            .appendingPathComponent("cyb-remux-empty-\(UUID().uuidString).mp4")
        FileManager.default.createFile(atPath: empty.path, contents: Data())
        defer { try? FileManager.default.removeItem(at: empty) }
        // 壊れたファイルは開けないので throw する。**黙って nil を返さない**
        // （「音声が無い」と「開けない」を混ぜると、呼び出し側が再エンコードへ
        //  落ちるべき場面で無音を出す）。
        XCTAssertThrowsError(try FFmpegAudioRemuxer.audioCodecName(of: empty))
    }

    /// 存在しないファイルは throw する。
    func testMissingFileThrows() {
        let missing = URL(fileURLWithPath: "/tmp/definitely-not-here-\(UUID().uuidString).mp4")
        XCTAssertThrowsError(try FFmpegAudioRemuxer.copyAudioStream(from: missing, to: missing))
    }

    /// 実素材（環境変数で渡す）。コーデックが読め、パケットが 1 つ以上書ける。
    func testCopiesAudioStreamFromRealSource() throws {
        guard let source = sourceURL else {
            throw XCTSkip("CYB_REMUX_TEST_SOURCE が未設定なので skip")
        }
        let codec = try FFmpegAudioRemuxer.audioCodecName(of: source)
        XCTAssertNotNil(codec, "音声トラックが読めない")

        let out = FileManager.default.temporaryDirectory
            .appendingPathComponent("cyb-remux-\(UUID().uuidString).m4a")
        defer { try? FileManager.default.removeItem(at: out) }

        let packets = try FFmpegAudioRemuxer.copyAudioStream(from: source, to: out)
        XCTAssertGreaterThan(packets, 0, "パケットが 1 つも書けていない")

        let size = (try FileManager.default.attributesOfItem(atPath: out.path)[.size] as? Int) ?? 0
        XCTAssertGreaterThan(size, 1024, "出力が小さすぎる（器だけできている）")
    }
}
