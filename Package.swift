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
let releaseTag = "v1.0.1"
let releaseBase = "https://github.com/cyber937/CYBFFmpeg/releases/download/\(releaseTag)"

// Checksums printed by make-xcframeworks.sh (release mode only).
// v1.0.1: CybFFmpegCore is now library-only (no bundled modulemap), so all
// artifacts were re-zipped — checksums differ from v1.0.0.
let checksums: [String: String] = [
    "CybFFmpegCore": "d062577159fc51db8c77c7236f66dc0d54de11ab18bc019c5b44a37bb0b2adee",
    "avcodec":       "61b339da25ea970be34c9373982a2661eca9be5e9c9f91b4d7fcfc23aa5ba8df",
    "avformat":      "5f2dbbe211b4632ce2647495802e65b7ded3e018c9b44b84f27a0bc4d6f8559a",
    "avutil":        "0c05a9a37df28bad155cfcd17ee8b2b9333002f6413066e987932c1febd2f0a6",
    "swscale":       "8bb448a0fb4a6a19b9bd546e081badb0b18e1607d6d597fe55adeb3fceaa80d4",
    "swresample":    "a43581e3aad316e6c283f5102eef77729c19b7c120c032b9c3f65ef92cc54dac",
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
