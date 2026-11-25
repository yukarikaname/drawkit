# Building DrawKit

The shells share one Rust core. Everything below builds a shell against that core; the macOS
package links the release static libraries from `target/release`.

## Requirements

- Rust stable (edition 2024).
- macOS shell: Xcode 27 with the macOS 27 SDK; the package uses Swift tools 6.4.
- iPad shell: Xcode 27 and the iOS 27 SDK (the build script selects the simulator or device SDK).
- Android shell: JDK 17, the Android NDK, compile SDK 37 and an arm64 device or emulator.
- Windows shell: .NET 8 SDK, Windows App SDK and the MSVC Rust target.

## macOS

```sh
cargo test                       # core, formats, figure and GPU tests
make app                         # dist/DrawKit.app
open dist/DrawKit.app
```

`make run` builds and opens it. The bundle is ad-hoc signed and self-contained: the Rust code is
linked statically, so `dist/DrawKit.app` can be copied to another Mac.

`DrawKit.xcodeproj` has two targets over the same sources: `DrawKit-macOS` and `DrawKit-iPad`.
The project links the release static libraries from `target/release`, so build the core first:

```sh
cargo build --release -p drawkit-core -p drawkit-gpu
xcodebuild -project DrawKit.xcodeproj -scheme DrawKit-macOS -configuration Debug build
```

`swift run --package-path app-macos` still works for the package-only path.

## iPad

`DrawKit-iPad` is the application target for the iPad shell. Its simulator slice links
`target/aarch64-apple-ios-sim/release`:

```sh
rustup target add aarch64-apple-ios-sim
cargo build --release --target aarch64-apple-ios-sim -p drawkit-core -p drawkit-gpu
xcodebuild -project DrawKit.xcodeproj -scheme DrawKit-iPad -configuration Debug \
    -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' build
```

The target is arm64-only because the Rust simulator library is arm64. For a device, add the
`aarch64-apple-ios` Rust target and select a development team; signing is configured in the
project. `./scripts/build-ios.sh` remains the icon-and-launch-image bundling path for the flat
build.

## Android

```sh
./scripts/build-android.sh              # JNI library and debug APK
adb install -r app-android/app/build/outputs/apk/debug/app-debug.apk
```

The project uses the Compose template from the Android CLI. The Android Gradle plugin, compile SDK
and NDK are taken from the local SDK.

## Windows

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc -p drawkit-core -p drawkit-gpu
New-Item -ItemType Directory -Force app-windows/native | Out-Null
Copy-Item target/x86_64-pc-windows-msvc/release/drawkit_core.dll app-windows/native/drawkit_core.dll
Copy-Item target/x86_64-pc-windows-msvc/release/drawkit_gpu.dll app-windows/native/drawkit_gpu.dll
dotnet run --project app-windows/DrawKit.WinUI.csproj -p:Platform=x64
```

See `app-windows/README.md` for the ARM64 variant and the current limits.

## CI and release

`.github/workflows/ci.yml` runs Rust format, clippy and tests; the macOS Swift shell; the Android
APK; and the Windows WinUI build.

Pushing a `v*` tag runs `.github/workflows/release.yml` and publishes an arm64 macOS zip. Windows
and Linux packages return with their native shells; the old self-drawn prototype binary is gone.
The macOS release needs an Xcode image with the macOS 27 SDK.
