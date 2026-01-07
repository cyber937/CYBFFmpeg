// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

// Absolute path to CYBFFmpeg package directory
// IMPORTANT: Update this path if the project location changes
let packageDir = "PACKAGE_ROOT_PATH"

// Path to Rust library (absolute path for Xcode compatibility)
let rustLibPath = "\(packageDir)/cyb-ffmpeg-core/target/release"

// Path to FFmpeg (Homebrew - supports both ffmpeg@7 and ffmpeg 8.x)
let ffmpegPath = "/opt/homebrew/opt/ffmpeg"

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
        .target(
            name: "CYBFFmpeg",
            dependencies: ["CybFFmpegC"],
            path: "Sources/CYBFFmpeg",
            exclude: ["CybFFmpegC"],
            linkerSettings: [
                // System frameworks
                .linkedFramework("VideoToolbox"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Security"),
                .linkedFramework("AudioToolbox"),
                // Rust static library (absolute path for Xcode compatibility)
                .unsafeFlags(["-L", rustLibPath]),
                .linkedLibrary("cyb_ffmpeg_core"),
                // FFmpeg libraries
                .unsafeFlags(["-L", "\(ffmpegPath)/lib"]),
                .linkedLibrary("avcodec"),
                .linkedLibrary("avformat"),
                .linkedLibrary("avutil"),
                .linkedLibrary("swscale"),
            ]
        ),
        .systemLibrary(
            name: "CybFFmpegC",
            path: "Sources/CYBFFmpeg/CybFFmpegC"
        ),
        .testTarget(
            name: "CYBFFmpegTests",
            dependencies: ["CYBFFmpeg"]
        ),
    ]
)
