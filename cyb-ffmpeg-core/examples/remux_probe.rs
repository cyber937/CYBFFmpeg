//! `remux_audio` を実素材で走らせて、時間とパケット数を出す。
//!
//! 使い方: cargo run --release --example remux_probe -- <入力> <出力.m4a>
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: remux_probe <input> <output.m4a>");
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    match cyb_ffmpeg_core::remux::audio_codec_name(input) {
        Ok(Some(c)) => println!("audio codec: {c}"),
        Ok(None) => { eprintln!("no audio stream"); std::process::exit(1); }
        Err(e) => { eprintln!("codec probe failed: {e}"); std::process::exit(1); }
    }
    let t = Instant::now();
    match cyb_ffmpeg_core::remux::remux_audio(input, output) {
        Ok(stats) => println!("packets: {} / elapsed: {:.3}s", stats.packets, t.elapsed().as_secs_f64()),
        Err(e) => { eprintln!("remux failed: {e}"); std::process::exit(1); }
    }
}
