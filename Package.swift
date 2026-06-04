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
let releaseTag = "v1.0.0"
let releaseBase = "https://github.com/cyber937/CYBFFmpeg/releases/download/\(releaseTag)"

// Checksums printed by make-xcframeworks.sh (release mode only).
let checksums: [String: String] = [
    "CybFFmpegCore": "e04cca868785fcb1c232730192ddc821db0169f52ee315616df7825d2ea6bce6",
    "avcodec":       "e9dcef31d22faa4524c4064f4e4e168a6ab41c221d1e19fcc755ffbcf6a675b6",
    "avformat":      "588255120195debaa39002032077dffcd2a629c106c18fc58232bb3cc7e02e32",
    "avutil":        "ca8da6221d8623f4371ff2b237b783229357631b29ad64fce695ef93a03a73b6",
    "swscale":       "f7df31a8c7b255a7d58923c12799b76946e49748c4a7ac79fad30055619ce042",
    "swresample":    "6b52f0c2e3133669a0cefaca879c446c114607a274ae9a2891c76bbad9fcc783",
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
        nativeBinary("CybFFmpegCore"),   // Rust static + headers/modulemap (module `CybFFmpegC`)
        nativeBinary("avcodec"),         // FFmpeg dynamic libs (LGPL — replaceable)
        nativeBinary("avformat"),
        nativeBinary("avutil"),
        nativeBinary("swscale"),
        nativeBinary("swresample"),
        // --- Swift layer (source) --------------------------------------------
        .target(
            name: "CYBFFmpeg",
            dependencies: [
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
