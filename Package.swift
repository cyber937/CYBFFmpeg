// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription
import Foundation

// =============================================================================
// CYBFFmpeg — native layer shipped as pre-built XCFrameworks.
//
// The heavy native artifacts (FFmpeg 7.1 dynamic libs + Rust static core) are
// frozen for v2 and packaged by ./make-xcframeworks.sh. Consumers therefore
// link ready-made binaries: no 30-60 min FFmpeg compile, no cargo build, and
// no sibling-clone source layout. Only the thin Swift layer below is built
// from source.
//
//   • Default (release mode): binaries resolve from a GitHub Release via
//     url + checksum.
//   • CYBFFMPEG_LOCAL=1: binaries resolve from ./dist/*.xcframework (produced
//     by ./make-xcframeworks.sh) — for developing/verifying the native layer
//     itself before cutting a Release.
//
// FFmpeg libs are kept DYNAMIC on purpose (LGPL v3.0 — the libraries must
// remain user-replaceable); only the Rust core (our code) is static.
// =============================================================================

let useLocalBinaries = ProcessInfo.processInfo.environment["CYBFFMPEG_LOCAL"] != nil

// Bump this when publishing a new Release; checksums come from make-xcframeworks.sh.
let releaseTag = "v1.0.2"
let releaseBase = "https://github.com/cyber937/CYBFFmpeg/releases/download/\(releaseTag)"

// Checksums printed by make-xcframeworks.sh (release mode only).
// v1.0.2: 音声の無変換コピー（`FFmpegAudioRemuxer`）を足した。Rust の core が
// 変わったので CybFFmpegCore は当然変わる。**FFmpeg の 5 本は中身を再ビルドして
// いないが、zip を作り直したのでチェックサムは変わる** —— 6 本すべて上げ直すこと。
// v1.0.1: CybFFmpegCore is now library-only (no bundled modulemap), so all
// artifacts were re-zipped — checksums differ from v1.0.0.
let checksums: [String: String] = [
    "CybFFmpegCore": "e91588eabd4b676dc6c7fe336d4f9a36552c72ee8a91e68d3f4fbd151462ea3e",
    "avcodec":       "b00bfa014b64e721058cba860b17eaccaded121fb70dd6efabc01e292a10e82e",
    "avformat":      "86575e75ac194d8a2245e6dae353c2669002abca9cf469ef213bbc60806937ec",
    "avutil":        "03a36345b5ec5379cbbca58e54ff2c560b10d624acb945a25e2c3ab2575e9a9b",
    "swscale":       "796901f78f253d08ad79938084893edc87f4346f3d8d653d0fdca9d7c0e547a8",
    "swresample":    "5de0dbe0b416611d031fbb441d38d9da0461a0e2afdd7528163699429dc595a8",
]

func nativeBinary(_ name: String) -> Target {
    if useLocalBinaries {
        return .binaryTarget(name: name, path: "dist/\(name).xcframework")
    }
    return .binaryTarget(
        name: name,
        url: "\(releaseBase)/\(name).xcframework.zip",
        checksum: checksums[name]!
    )
}

let package = Package(
    name: "CYBFFmpeg",
    platforms: [.macOS(.v14)],
    products: [
        .library(
            name: "CYBFFmpeg",
            targets: ["CYBFFmpeg"]
        ),
    ],
    targets: [
        // --- Native layer (pre-built binaries) -------------------------------
        nativeBinary("CybFFmpegCore"),   // Rust static lib ONLY (no headers)
        nativeBinary("avcodec"),         // FFmpeg dynamic libs (LGPL — replaceable)
        nativeBinary("avformat"),
        nativeBinary("avutil"),
        nativeBinary("swscale"),
        nativeBinary("swresample"),
        // C interop module for the Rust FFI. Kept as a SOURCE systemLibrary
        // (not bundled in the CybFFmpegCore XCFramework) so its module.modulemap
        // is referenced in place and never copied to the consuming app's shared
        // `include/` — which would collide with another static-lib XCFramework's
        // modulemap (e.g. KirinukiCore) as "Multiple commands produce
        // include/module.modulemap". The actual symbols come from the
        // `CybFFmpegCore` binary above.
        .systemLibrary(
            name: "CybFFmpegC",
            path: "Sources/CYBFFmpeg/CybFFmpegC"
        ),
        // --- Swift layer (source) --------------------------------------------
        .target(
            name: "CYBFFmpeg",
            dependencies: [
                "CybFFmpegC",
                "CybFFmpegCore",
                "avcodec",
                "avformat",
                "avutil",
                "swscale",
                "swresample",
            ],
            path: "Sources/CYBFFmpeg",
            exclude: ["CybFFmpegC"],
            linkerSettings: [
                .linkedFramework("VideoToolbox"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Security"),
                .linkedFramework("AudioToolbox"),
            ]
        ),
        .testTarget(
            name: "CYBFFmpegTests",
            dependencies: ["CYBFFmpeg"]
        ),
    ]
)
