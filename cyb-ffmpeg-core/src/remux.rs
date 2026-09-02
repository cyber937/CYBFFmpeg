//! 音声ストリームの**無変換コピー**（demux → mux）。
//!
//! # なぜ要るか（kirinuki-ai #1765）
//!
//! 長尺の取り込みで「Extracting audio...」が尺に比例して伸びる。89.5 分の画面収録で
//! **90 秒**（内蔵 SSD）。同じことを `ffmpeg -vn -c:a copy` でやると **1.14 秒**。
//!
//! 原因は `AVAssetReader.copyNextSampleBuffer()` の 1 回あたりのコストで、
//! 実測 **534 μs**。ffmpeg は同じ内容を 1 パケット **5 μs** で処理しており **107 倍**違う。
//! やっていること（AAC のビットストリームをそのまま別コンテナへ書く）は同一で、
//! 1 個あたりのコストだけが 2 桁違う。この mp4 は音声がビデオ 1 フレーム（33 ms）ごとの
//! チャンクに分かれ、89.5 分ぶんで **161,132 個**になる。
//!
//! 🚨 **AVFoundation の中に回避策は無い**（実測済み）:
//! - `requestMediaDataWhenReady` に置換 → 94.1 秒（改善なし）
//! - 音声のみの `AVMutableComposition` を passthrough 書き出し → 122.8 秒（悪化）
//!
//! # 何をするか
//!
//! 入力から音声ストリームを 1 本選び、**パケットを再エンコードせずに**出力へ書く。
//! `ffmpeg -vn -c:a copy` と同じ。デコードもエンコードもしないので、CPU は
//! 「読んで書く」だけになる。
//!
//! # 出力の等価性（kirinuki-ai #1765 で実測済み）
//!
//! - 生 AAC ペイロードの md5 が現行出力と**完全一致** —— 文字起こしに渡る音は 1 ビットも変わらない
//! - デコードした PCM が現行出力の全長（10,584,000 バイト）まで一致。差は末尾に
//!   **16 サンプル（0.36 ms）**余分に付くだけ（Apple のエンコーダディレイ情報
//!   `iTunSMPB` が入らないため、その分がトリムされない）
//! - `AVAssetReader` で開ける（波形生成に影響しない）
//!
//! # 適用条件
//!
//! **元の音声が既に目的のコーデックであること。** そうでなければ呼び出し側が
//! 従来どおり再エンコードに落ちる。ここは判定しない —— 判定材料（コーデック名）は
//! [`audio_codec_name`] で返すので、方針は呼び出し側が持つ。

use std::path::Path;

use ffmpeg_next as ffmpeg;
use ffmpeg::media::Type as MediaType;

use crate::error::Error;

/// 無変換コピーの結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemuxStats {
    /// 書いたパケット数。**0 は失敗として扱う** —— 音声トラックは在るのに
    /// 1 つも書けなかったなら、出力は無音の器でしかない。
    pub packets: u64,
}

/// 入力の音声ストリームのコーデック名を返す（`aac` / `pcm_s16le` など）。
///
/// 呼び出し側が「無変換コピーでよいか」を決めるための材料。音声が無ければ `None`。
pub fn audio_codec_name(input: &Path) -> Result<Option<String>, Error> {
    ffmpeg::init().map_err(|e| Error::InvalidFormat(e.to_string()))?;
    let ictx = ffmpeg::format::input(&input).map_err(|_| Error::FileNotFound(input.to_path_buf()))?;
    let Some(stream) = ictx.streams().best(MediaType::Audio) else {
        return Ok(None);
    };
    Ok(Some(format!("{:?}", stream.parameters().id()).to_lowercase()))
}

/// 音声ストリームを 1 本、**無変換で** `output` へ書き出す。
///
/// 出力コンテナは拡張子から決まる（`.m4a` → ipod muxer）。
///
/// 🚨 **タイムスタンプは入力の time_base から出力の time_base へ変換する。**
/// これを忘れると尺が伸び縮みする（同じ数値を別の単位で書くことになる）。
pub fn remux_audio(input: &Path, output: &Path) -> Result<RemuxStats, Error> {
    ffmpeg::init().map_err(|e| Error::InvalidFormat(e.to_string()))?;

    let mut ictx = ffmpeg::format::input(&input).map_err(|_| Error::FileNotFound(input.to_path_buf()))?;
    let (in_index, in_time_base, parameters) = {
        let stream = ictx
            .streams()
            .best(MediaType::Audio)
            .ok_or_else(|| Error::InvalidFormat("no audio stream".to_string()))?;
        (stream.index(), stream.time_base(), stream.parameters())
    };

    let mut octx =
        ffmpeg::format::output(&output).map_err(|e| Error::InvalidFormat(e.to_string()))?;
    {
        let mut ost = octx
            .add_stream(ffmpeg::encoder::find(ffmpeg::codec::Id::None))
            .map_err(|e| Error::InvalidFormat(e.to_string()))?;
        ost.set_parameters(parameters);
        // 🔑 codec_tag は入力コンテナのもの。出力コンテナでは意味が違うことがあるので
        // 0 にして muxer に選ばせる（ffmpeg 公式の remux 例と同じ）。
        unsafe {
            (*ost.parameters().as_mut_ptr()).codec_tag = 0;
        }
    }

    octx.write_header().map_err(|e| Error::InvalidFormat(e.to_string()))?;
    let out_time_base = octx
        .stream(0)
        .ok_or_else(|| Error::InvalidFormat("output stream missing".to_string()))?
        .time_base();

    let mut packets = 0u64;
    for (stream, mut packet) in ictx.packets() {
        if stream.index() != in_index {
            continue;
        }
        packet.rescale_ts(in_time_base, out_time_base);
        packet.set_position(-1);
        packet.set_stream(0);
        packet
            .write_interleaved(&mut octx)
            .map_err(|e| Error::DecodeFailed(e.to_string()))?;
        packets += 1;
    }
    octx.write_trailer().map_err(|e| Error::InvalidFormat(e.to_string()))?;

    if packets == 0 {
        return Err(Error::DecodeFailed(
            "audio stream produced no packets".to_string(),
        ));
    }
    Ok(RemuxStats { packets })
}
