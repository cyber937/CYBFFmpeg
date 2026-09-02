// FFmpegAudioRemuxer.swift
// CYBFFmpeg
//
// 音声ストリームの無変換コピー（demux → mux）。

import Foundation
import CybFFmpegC

/// 音声ストリームを**再エンコードせずに**別コンテナへ書き出す。
///
/// `ffmpeg -vn -c:a copy` と同じことをする。デコードもエンコードもしないので、
/// CPU は「読んで書く」だけになる。
///
/// # なぜ在るか（kirinuki-ai #1765）
///
/// `AVAssetReader` で同じことをすると、`copyNextSampleBuffer()` 1 回あたり **534 μs**
/// かかる。89.5 分の画面収録では音声チャンクが **161,132 個**あり、合計 **90 秒**（内蔵 SSD）。
/// 尺にほぼ比例して伸びるので、長尺ほど効く。
///
/// 実測（同じ 89.5 分 / 410MB の素材・MacBook Air M4）:
///
/// | 経路 | 内蔵 SSD | 外付け HDD |
/// |---|---|---|
/// | `AVAssetReader`（従来）| 90.0 s | 114.5 s |
/// | **`FFmpegAudioRemuxer`** | **0.64 s** | **4.4 s** |
///
/// 出力は `ffmpeg -c:a copy` と**生 AAC ペイロードの md5 が一致**する
/// （`01288a2dcecd4751d512abbb64a43266`・実測）。
///
/// # 使い方
///
/// ```swift
/// // 入力が既に目的のコーデックかを確かめてから呼ぶ。
/// if try FFmpegAudioRemuxer.audioCodecName(of: url) == "aac" {
///     let packets = try FFmpegAudioRemuxer.copyAudioStream(from: url, to: outURL)
/// }
/// ```
///
/// 🚨 **コーデック変換はしない。** 入力の音声がそのまま出る。変換が要るなら
/// 呼び出し側が別の経路（再エンコード）へ落とすこと。
public enum FFmpegAudioRemuxer {

    /// 入力の音声ストリームのコーデック名（`aac` など）。音声が無ければ `nil`。
    ///
    /// 「無変換コピーでよいか」を決めるための材料。
    public static func audioCodecName(of url: URL) throws -> String? {
        var buffer = [CChar](repeating: 0, count: 64)
        let result = url.path.withCString { path in
            cyb_audio_codec_name(path, &buffer, UInt(buffer.count))
        }
        if result == CYB_RESULT_ERROR_CODEC_NOT_SUPPORTED {
            return nil   // 音声トラックが無い
        }
        try Self.check(result, url: url)
        return String(cString: buffer)
    }

    /// 音声ストリームを無変換で `destination` へ書き出し、書いたパケット数を返す。
    ///
    /// 出力コンテナは拡張子から決まる（`.m4a` → M4A）。
    ///
    /// 🚨 **0 パケットは失敗として扱う**（Rust 側で弾く）。音声トラックは在るのに
    /// 1 つも書けなかったなら、出力は無音の器でしかない。
    @discardableResult
    public static func copyAudioStream(from source: URL, to destination: URL) throws -> UInt64 {
        var packets: UInt64 = 0
        let result = source.path.withCString { input in
            destination.path.withCString { output in
                cyb_remux_audio(input, output, &packets)
            }
        }
        try Self.check(result, url: source)
        return packets
    }

    // MARK: - Internals

    private static func check(_ result: CybResult, url: URL) throws {
        guard result != CYB_RESULT_SUCCESS else { return }
        let message = cyb_get_last_error().map { String(cString: $0) } ?? "Unknown error"
        switch result {
        case CYB_RESULT_ERROR_FILE_NOT_FOUND:
            throw FFmpegError.fileNotFound(url)
        case CYB_RESULT_ERROR_INVALID_FORMAT:
            throw FFmpegError.invalidFormat(message)
        case CYB_RESULT_ERROR_CODEC_NOT_SUPPORTED:
            throw FFmpegError.codecNotSupported(message)
        case CYB_RESULT_ERROR_DECODE_FAILED:
            throw FFmpegError.decodeFailed(message)
        case CYB_RESULT_ERROR_MEMORY:
            throw FFmpegError.memoryError
        default:
            throw FFmpegError.unknown(Int32(result.rawValue))
        }
    }
}
