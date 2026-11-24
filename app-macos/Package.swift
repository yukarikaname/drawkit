// swift-tools-version: 6.4
import PackageDescription

// The Rust side is two static libraries: the painting core and the GPU figure. Linker flags resolve
// from the build directory, not from this manifest, so the library path is derived from the
// manifest's own location. A checkout elsewhere, or CI, needs no edit.
//
// The package targets the macOS 27 and iOS 27 SDKs. The executable below is the macOS shell; the
// iPad target lives in app-ios and shares the Rust libraries and the UI specification.
import Foundation

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let rustLibraryPath = packageDirectory + "/../target/release"

let package = Package(
    name: "DrawKit",
    platforms: [.macOS(.v27), .iOS(.v27)],
    targets: [
        .executableTarget(
            name: "DrawKit",
            path: "Sources/DrawKit",
            linkerSettings: [
                .unsafeFlags([
                    "-L\(rustLibraryPath)",
                    "-ldrawkit_core",
                    "-ldrawkit_gpu",
                    "-liconv",
                ]),
                .linkedFramework("Metal"),
                .linkedFramework("QuartzCore"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("Foundation"),
                .linkedFramework("CoreFoundation"),
            ]
        )
    ],
    swiftLanguageModes: [.v6]
)
